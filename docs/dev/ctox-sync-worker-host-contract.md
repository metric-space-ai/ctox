# Worker-Aufnahme: Vertrag zwischen Workjet-Host und CTOX Sync

Stand 2026-09-08. Dies dokumentiert den aktuellen Quellvertrag und die noch
fehlenden Produktanschlüsse. Es ist keine Freigabe des SSH-/QR-Ablaufs.
Die Abnahme und ihre Testergebnisse stehen in
[ctox-sync-core-offensive.md](ctox-sync-core-offensive.md).

## Bestehender lokaler Anschluss

Workjet verwendet `apps/server/src/workjet/sync/WorkjetSyncIpc.ts`:

```ts
requestSyncAuthority(endpoint: string, input: SyncIpcRequest,
                     signal?: AbortSignal): Promise<SyncIpcResponse>
```

Die Typen, Effect-Schemas und Konstanten stammen aus `@t3tools/contracts/ctoxSync`.
Quelle ist `src/core/rxdb/tests/fixtures/ctox_execution_contract.json` in CTOX;
`src/core/sync/tools/generate-contracts.mjs` generiert beide Sprachen.
Keine zweite Strukturdefinition im Host und keine HTTP-Mailbox einführen.

`endpoint` kommt vom laufenden nativen Host über `ipc_endpoint()`, nach
erfolgreichem `NativeSyncSession::attach_execution` beziehungsweise
`attach_worker`. Der Host übergibt einen absoluten privaten lokalen Socketpfad.
Der native Listener prüft Unix-Benutzer und private Dateirechte. Windows hat
noch keinen nativen Listener; die Named-Pipe-Unterstützung des Clients allein
hebt diese Grenze nicht auf. Einen Socketpfad aus einem QR-Code, Browserfeld
oder einer fremden Installation darf der lokale Host nicht als eigenen
Runtime-Endpunkt übernehmen.

Der Host hält die Session einschließlich Attachment am Leben, überwacht
`wait_stopped()` und wartet beim Stoppen auf `shutdown()`. Er entzieht danach
den Endpunkt aus seinem Runtime-Zustand. Ein Ansichtswechsel beendet diese
Session nicht. Auf Mobile läuft der Node-IPC-Aufruf auf dem autorisierten
Ausführungs-/Admin-Host; die mobile Oberfläche kann keinen entfernten Unix-
Socket direkt öffnen. Der produktive Weg zu diesem Host ist noch anzuschließen.

## Native Identität ohne Business-OS-Daten

`NativeSyncOptions.database` ist die explizite, vom Host gehaltene RxDB-Identität.
`collections` darf leer sein; vorhandene Collections müssen zu genau dieser
Datenbank gehören. Diese Datenbankidentität ersetzt keinen signierten
Geräteschlüssel. Der Host muss für einen Worker oder eine reine
Koordinationsstimme keine künstliche Business-OS-Collection erzeugen.
Ein leerer Peer meldet im bestehenden Protokoll `collection: null` und explizite
leere `collectionSchemas`-/`collectionCheckpoints`-Objekte. Auf Daten-Peers
entstehen nur für gemeinsam angebotene Collections Replikationszustände.
Eine fehlende Schema-Antwort erhält dadurch keine zusätzliche Berechtigung.

Diese Trennung liefert keine produktive Netzwerk-Konfiguration, keinen
Signaling-Grant und keinen Aufnahmebeleg. Der Host muss weiterhin seine
persistierte Identität und die drei geprüften Voter bereitstellen.

Die Signaling-Adressen dieser Voter sind keine dauerhafte Konfiguration mehr:
`routes` darf als leerer Starthinweis übergeben werden. Der native Host prüft
einen frischen, signierten Adressnachweis gegen die drei konfigurierten Schlüssel,
den Scope, die Anfrage-Nonce und die aktuelle Verbindung. Nach einem Wechsel
der Signaling-Adresse ersetzt dieser Nachweis die Route. Er bestätigt weder
Mitgliedschaft noch Quorum oder Ausführbarkeit. Der versionierte native
Kontrollaufruf `ctox.sync.authority.route.v1` verwendet den vorhandenen signierten
Envelope mit eigenen Request-/Reply-Kennungen; er ist keine Workjet-IPC-Operation.

## Persistierte native Host-Konfiguration

`ctox_sync::host_config` liest und schreibt einen versionierten Singleton in
`ctox_sync_host` innerhalb der vom Host bereitgestellten Runtime-SQLite-Datenbank.
Die vorhandenen Runtime-Tabellen bleiben erhalten. Der Datensatz pinnt Scope,
lokale Node-ID, Rolle und Public-Key sowie genau drei verschiedene Voter mit
ihren Ausführungs-/Datenfähigkeiten. Änderungen dieser Bindung werden auch bei
einem erneuten Speichern in einer unmittelbaren Transaktion abgelehnt; sie
benötigen einen expliziten Migrationsweg. Raft-Zeitparameter können für einen
späteren Neustart aktualisiert werden.

`voter_options` und `worker_options` prüfen zusätzlich den aus dem lokalen
Secret-Store gelieferten Schlüssel und erzeugen die bestehenden nativen
Attachment-Optionen. Das private lokale IPC-Verzeichnis bleibt ein gesondertes
Startargument des Hosts: Der vollständige dauerhafte Datenpfad kann länger als
ein zulässiger Unix-Socketpfad sein. Der bestehende Listener prüft weiterhin
Pfadlänge, Benutzer, Dateirechte und exklusiven Besitz. Die Konfiguration speichert
weder diesen Schlüssel noch
Zugangstoken, flüchtige Routen oder einen aktiven IPC-Endpunkt. Die Route-Maps
starten leer. Der dedizierte Raum heißt `ctox-execution:<scope>`; er ersetzt
keine Business-OS-Session und darf keine Business-OS-Zugangsdaten übernehmen.
Worker erhalten keinen Raft-Speicherpfad. Eine gespeicherte Worker-Konfiguration
ist ausdrücklich kein bestätigter Mitgliedschaftsbeleg.

Die Host-Setup-Typen stammen jetzt ebenfalls aus der kanonischen Fixture:
`SyncHostConfiguration`, `SyncHostMember`, `SyncHostTiming`, `ExecutionPeer`,
`SyncHostTransport` und `SyncHostIceServer`. Rust-Definitionen, TypeScript und
Effect-Schemas werden gemeinsam generiert. Die bisherigen handgeschriebenen
Rust-Strukturen sind durch diese Typen ersetzt. Die Formprüfung ersetzt nicht
die native Prüfung der unterschiedlichen Pins und zulässigen Mitgliedschaft.

## Konkreter CTOX-Host-Anschluss

Der CTOX-Service ruft vor seinen Business-OS-Workern `start_if_configured` auf.
Ein konfigurierter Host muss erfolgreich starten; ein Fehler bricht den
Service-Start ab. Ohne Konfiguration bleibt dieser Anschluss inaktiv. Dieselbe
native Laufzeit ist als `ctox sync run --root <installation>` eigenständig
prüfbar. Sie startet keine Business-Worker und öffnet keine Business-Collections.
Der volle CTOX-Service-Ablauf ist damit noch nicht abgenommen.

Lokale administrative CLI-Schritte:

1. `ctox sync init --root <installation>` erzeugt eine stabile lokale Identität;
   ein erneuter Aufruf behält sie. `identity` gibt ausschließlich den Public-Key
   aus. `import-key <public-identity>` übernimmt Base64-PKCS#8 über stdin und
   prüft den erwarteten Schlüssel; eine andere bestehende Identität wird nicht
   ersetzt. Insbesondere bleiben Workjet-Schlüssel im PKCS#8-v1-Format erhalten.
2. `configure` liest die öffentliche `SyncHostConfiguration` als JSON über stdin
   und speichert sie in `runtime/ctox-runtime.sqlite3`.
3. `transport` liest `SyncHostTransport` über stdin und speichert es verschlüsselt
   unter dem Secret-Scope `ctox-sync-host`. `signalingUrls` und `iceServers`
   sind explizit anzugeben. ICE-Einträge enthalten `urls`, `username` und
   `credential`, bei Bedarf leere Strings. Leere ICE-Konfiguration wählt keine
   öffentlichen STUN-Defaults; Signaling kann sein authentifiziertes ICE-Bootstrap
   liefern. Neue URLs werden beim nächsten Reconnect gelesen, ICE beim Neustart.
4. `run` hält Session und privaten lokalen IPC-Listener bis SIGINT/SIGTERM.
   `status` prüft den veröffentlichten Listener mit dem generierten `hello`-
   Vertrag gegen Node, Scope und Protokoll. Ein aktiver Listener bedeutet keine
   bestätigte Mitgliedschaft, erreichbare Mehrheit oder ausführbaren Harness.

Der Host hält vor dem Öffnen seiner Stores einen exklusiven Prozess-Lock.
Ein zweiter Host für denselben Installationsroot wird abgelehnt. Der private
Socketpfad wird getrennt vom dauerhaften Datenpfad erzeugt; der veröffentlichte
Deskriptor wird bei Shutdown entzogen und niemals aus einem Invite übernommen.
Bei unerwartetem Ende der nativen Laufzeit verschwindet der Listener. Es gibt
keine zusätzliche Retry-/Supervisor-Schleife neben dem bestehenden Sync-Lifecycle.

Der Transport verlangt die passende native Rolle und TLS, ausgenommen lokales
Loopback-Signaling. Bekannte Business-OS-Grant-Felder werden abgewiesen; der Host
liest keine Business-OS-Zugangsdaten als Ersatz. Die produktive Signaling-
Zulassung dieses Ausführungsnetzes ist weiterhin offen. Der aktuelle öffentliche
Business-OS-Signaling-Vertrag reicht dafür nicht aus. Die CLI ist deshalb noch
kein abgeschlossener öffentlicher SSH-/QR-Aufnahmeablauf. Windows-Hosting bleibt
ausdrücklich nicht unterstützt.

## Operationen und genaue Bedeutung

Jede Anfrage enthält `{ version, requestId, operation }`. `version` ist
`CTOX_SYNC_IPC_PROTOCOL_VERSION` (derzeit 1), `requestId` ist nicht leer und
höchstens 256 Zeichen lang. Der Client übernimmt Framing, Schema- und
Antwortkorrelation, 64-KiB-Grenze sowie die Zehn-Sekunden-Deadline.

| Operation | Ort / Voraussetzung | Erfolgsantwort und Grenze |
| --- | --- | --- |
| `{ type: 'hello' }` | Lokaler voter- oder worker-Host | `ready { nodeId, scopeId, protocolVersion }` identifiziert ausschließlich den lokalen Listener. Kein Quorum-, Mitgliedschafts-, Verbindungs- oder Ausführungsnachweis. Der aufrufende Host muss Node und Scope mit seiner erwarteten Konfiguration vergleichen. |
| `{ type: 'admitWorker', worker }` | Lokaler bestätigter Voter, nach Admin-Autorisierung und Nachweis des Worker-Schlüssels | `workerApplied { worker }` bestätigt die gespeicherte Aufnahmeentscheidung. `workerReplayed { worker }` gibt den historischen Beleg derselben Anfrage zurück. Beides beweist keine aktuelle Erreichbarkeit oder Harness-Bereitschaft. |
| `{ type: 'revokeWorker', nodeId }` | Lokaler bestätigter Voter, nach Admin-Autorisierung | `workerApplied` / `workerReplayed`, mit `revoked: true` im zugehörigen Beleg. Eine einmal verwendete Node-ID wird nicht erneut vergeben. |
| `{ type: 'workerMembership', nodeId }` | Voter: Worker-Abfrage; Worker: ausschließlich eigener Eintrag | `workerMembership { nodeId, worker }` mit aktuellem Eintrag oder `null`, nach Quorum-Abfrage. Enthält auch Widerrufe. Native Quorum-/RPC-Prüfungen, echter Workjet-Client über Unix-Socket und die ergänzte native WebRTC-Prüfung bestanden; produktiver Host-Anschluss offen. Kein Erreichbarkeits- oder Harness-Nachweis. |
| `{ type: 'create', spec }` | Lokaler Executor | `applied` / `replayed` mit Spec und Ownership; der Besitzer ist der lokale Actor. Keine Remote-Dispatch-API für einen beliebigen UI-Zielhost. |
| `{ type: 'validate', jobId, ownership }` | Besitzer des Auftrags | `authorized { spec, ownership }` folgt einer Quorum-Prüfung von aktiver Mitgliedschaft und aktuellem Besitz. Kein allgemeiner Worker-Readiness-Test, kein Auftrag darf nur für eine Statusanzeige angelegt werden. |
| `beginEffect`, `completeEffect`, `stop` | Aktueller Executor, jeweils mit `jobId` und `ownership`; Effekte zusätzlich mit `effectId` | Bestehender Ausführungsvertrag; erst zusammen mit Supervisor-, Gateway- und Tool-Grenzen produktiv integrieren. |

`worker` ist `{ nodeId, identity, dataReplica, revoked }`. Node-IDs sind positive
JavaScript-sichere Ganzzahlen; `identity` ist der geprüfte Ed25519-Public-Key,
kein Gerätename und keine Signaling-Adresse. Bei Aufnahme ist `revoked: false`.
`dataReplica` erfordert eine vom Host geprüfte Daten-Peer-Fähigkeit und ist kein
frei wählbares Versprechen der UI. Es bleiben genau drei Voter; zusätzliche
Worker besitzen keinen Raft-Store und können keine Mitglieder aufnehmen.

## Wiederholung und unbekannte Ergebnisse

Der Host persistiert Request-ID, Ziel-Voter, Scope und unveränderte Operation
vor dem ersten mutierenden Aufruf. Nach Timeout, Abbruch, Socketfehler oder
`unavailable` ist das Ergebnis unbekannt: keine neue ID ausstellen, keinen
Erfolg anzeigen und keine Gegenoperation als vermeintlichen Rollback senden.
Die Wiederholung verwendet denselben lokalen Voter/Actor und dieselben Daten.
Der Actor ist Teil des nativen Request-Fingerprints; ein anderer Voter mit
identischer Request-ID ist daher kein identischer Replay. Einen automatischen
Wechsel des Admin-Hosts muss ein zukünftiger Aufnahmevertrag ausdrücklich
lösen, statt den vorhandenen Actor-Schutz zu lockern.

`rejected` ist ein explizites negatives Ergebnis. Eine Antwort muss zusätzlich
zur bereits geprüften Request-ID zum erwarteten Operationstyp, Worker-Schlüssel,
Node und Widerrufsstatus passen. `AlreadyExists` ist kein Aufnahmenachweis.

Besonders wichtig: Ein alter `workerReplayed`-Aufnahmebeleg kann `revoked: false`
enthalten, obwohl ein späterer Befehl den Worker bereits widerrufen hat. Das ist
korrekte historische Idempotenz und darf weder den aktuellen Widerruf überschreiben
noch den Worker wieder aktivieren. Die ergänzte Operation `workerMembership`
fragt den gegenwärtigen Zustand nach einer linearisierbaren Quorum-Abfrage ab.
Ihre Request-ID wird nicht als Mutationsbeleg gespeichert: Eine wiederholte
Abfrage darf einen inzwischen geänderten Zustand zurückgeben. Die native
Kontrollversion steigt dafür auf 5; ältere Kontrollversionen werden abgelehnt.
Rust, TypeScript und Effect-Schemas sind gemeinsam aus der Fixture generiert.
Die folgenden Zahlen sind historische Ergebnisse vor der Korrektur der
Transport-Parität. Sie gelten nicht als aktuelle Freigabe. Die aktuelle native
und vollständige Host-Abnahme mit Messwerten steht in
[Native-Transport-Parität](ctox-sync-native-transport-parity-20260908.md).

Historisch bestanden die drei gezielten nativen Worker-/Quorum-Tests (20,87 Sekunden),
einschließlich historischem Aufnahmebeleg, aktuellem Widerruf, fremden Abfragen,
Neustart, isoliertem alten Leader und Shutdown. Diese Tests verwenden signierte
RPCs über den Test-Bus und direkten IPC-Dispatch. Anschließend besteht die vollständige vorhandene
Cluster-Regressionsprüfung mit demselben neuen Testprogramm **11/11**
(23,37 Sekunden), einschließlich der bestehenden Unix-Socket- und echten
Workjet-IPC-Client-Fälle. Danach besteht die um die neue Mitgliedschaftsabfrage
erweiterte tatsächliche Workjet-Client-Fixture über den privaten Unix-Socket
**1/1** (11,10 Sekunden). Sie prüft unbekannten, aktiven und widerrufenen Zustand,
historischen Aufnahme-Replay, unveränderte Read-Request-ID sowie Quorumverlust
und Host-Stopp. Die anschließende vollständige aktuelle Sync-Prüfung besteht
**54/54** einschließlich **8/8 echter WebRTC-Szenarien** (18,87 Sekunden).
Die Worker-Fixture bestätigt aktuelle Mitgliedschaft nach Aufnahme und nach
Wiederverbindung mit neuer Signaling-Adresse sowie den aktuellen Widerruf trotz
historischem Aufnahme-Replay. Die bisherige 60-Sekunden-Deadline bleibt
unverändert. Produktiver Host-Anschluss und SSH-/QR-Aufnahme bleiben offen.


## Sichtbare Aufnahmeabläufe und Abnahmekriterien

Quellstand der aktuellen Web-/Desktop-Oberfläche: `WorkjetComputerEditor`
verlangt in `saveWorkjetComputerDraft` eine bestehende `environmentId`.
`WorkjetComputersSettingsView` speichert damit einen Computereintrag in der
Workjet-Konfiguration. Der Knopf „Add computer“ bestätigt keine native
Sync-Mitgliedschaft. Die darunter eingebundene `ComputerProvisioningSection`
hat einen eigenen Ablauf `target → fingerprint → preflight → components →
operation` über die Desktop-Bridge; ohne die benötigten Bridge-Methoden wird
sie ausgeblendet. Installation, Verbindungskatalog und bestätigte Aufnahme
sind daher gegenwärtig noch getrennte Produktanschlüsse. Diese Feststellung
stammt aus dem Quellcode, nicht aus einer bestandenen UI-Prüfung.

Die folgenden vollständigen Abläufe bleiben offen. Ihre ausführbaren
`WorkjetUiStoryV1`-Dateien erhalten die tatsächlichen sichtbaren Locators und
den konkreten isolierten Fixture-Start, sobald der produktive Host-Anschluss
vorliegt. Keine erfundenen Erfolgsanzeigen und keine versteckten IPC-Aufrufe
als Ersatz für fehlende UI-Schritte verwenden.

| Story-ID | Sichtbarer Ablauf | Erforderlicher Nachweis |
| --- | --- | --- |
| `worker.add.ssh` | Computers → Install on a computer → Remote over SSH → Host prüfen → Installation → Netzwerkaufnahme | Erwarteter Rechner und Schlüssel; aktuelle bestätigte Mitgliedschaft; Rechner auf Desktop und Mobile sichtbar; tatsächlich dort ausgeführter Auftrag. Eine erfolgreiche Installation allein beendet die Story nicht. |
| `worker.add.link` | Auf manuell installierter Workjet-Instanz QR/Link erzeugen; in Desktop beziehungsweise Mobile übernehmen und dem vorgesehenen Netzwerk zuordnen | Netzwerk und Identität stimmen; Aufnahme und echte Ausführung funktionieren bei gesperrtem direkten Admin-/HTTP-Zugang zum Worker über Signaling und WebRTC. |
| `worker.rejoin` | Aufgenommenen Worker neu starten beziehungsweise seine Verbindung unterbrechen und wiederherstellen | Derselbe Worker ohne doppelten Computereintrag; aktuelle Mitgliedschaft; wiederhergestellte Verbindung und sichtbarer Auftragsfortschritt auf beiden Oberflächen. Die native Signaling-Adresse darf wechseln. |
| `worker.revoked` | Worker entfernen; Aufnahmeantwort verzögert zustellen beziehungsweise Client neu laden | Widerruf bleibt sichtbar und wirksam; historischer Aufnahmebeleg aktiviert den Worker nicht erneut; keine neue autorisierte Ausführung. |

Alle Stories benötigen isolierte, bereinigte Testinstanzen, Reload-/Restart-
Nachweise, unveränderten Instanz-Scope und sichtbare Ergebnisse nach jedem
Schritt. Kritische UI-Abnahme folgt dem Workjet-UI-Testing-Vertrag mit
maskierter Aufzeichnung und unabhängiger Prüfung. Die nativen Quorum-, IPC-
und WebRTC-Tests bleiben separate Nachweise und ersetzen diese Stories nicht.

## Verifizierte Produktgrenzen am 2026-09-08

Workjets IPC-Client ist mit PR #32 auf main
1a81eabec00fa262c36d72970376f2d09da6a48f enthalten. Seine Aufrufer in
apps/packages sind weiterhin ausschließlich Tests. Der vollständige CTOX-Host
besteht inzwischen die Vier-Prozess-Abnahme in Run 34204358510; die
[Messwerte und Build-Identität](ctox-sync-native-transport-parity-20260908.md)
sind erhalten. SSH-/QR-Aufnahme und echte Harness-Ausführung wurden dort nicht
ausgeführt.

Die Quellprüfung des sauberen Broker-main
[1cebb53e0cf081fceeec7633f67e17223d31b31b](https://github.com/mkh-welsch/ctox-dev/blob/1cebb53e0cf081fceeec7633f67e17223d31b31b/cloudflare-signaling/src/index.js)
zeigt drei konkrete Anschlusslücken; das ist kein Nachweis des aktuell
ausgerollten Brokers:

- KNOWN_ROLES enthält workjet_executor nicht. normalizeRole kann unbekannte
  Rollen aus dem client-Label ableiten. Dieser Ersatz darf keine native
  Ausführungsrolle oder deren Berechtigung begründen.
- roleBoundAuthContext verlangt ctox-role-bound-v1 mit Browser-/Native-Hash.
  Nur ctox_instance benutzt den Native-Hash; alle anderen Rollen benutzen den
  Browser-Hash. Die bloße Ergänzung einer Worker-Rolle wäre deshalb kein
  eigenständiger Ausführungsnetz-Vertrag. Der native HostTransport lehnt diese
  Business-OS-Felder ausdrücklich ab.
- instanceIdFromBusinessOsRoom bindet nur ctox-business-os-Räume an instance_id.
  Für ctox-execution:scope liefert es keinen Scope; die vorhandene Join-Prüfung
  überspringt dann diesen Abgleich. Ein eigener Vertrag muss Identität, Rolle
  und Ausführungsnetz ausdrücklich binden, ohne Business-OS-Zulassung zu ändern.

Auf Workjet-main führt DesktopComputerProvisioner.start die Installation
weiterhin mit Effect.forkDetach aus und hält Operationen in einer lokalen Map.
Nach einem Desktop-Prozessneustart existiert dieser Aufnahme-/Installationsstand
nicht mehr. Ein get-not-found ist kein Abschlussbeleg und darf keine neue
Mutation mit neuer Request-ID auslösen. Die anschließende Registry-Bindung
verwendet Business-OS-Invites beziehungsweise addSshManagedInstance; sie führt
kein admitWorker aus. Der gemeinsame produktive Aufnahmevorgang benötigt
daher einen dauerhaften Besitzer samt identischer Wiederholungs-ID und die
bestätigte native Entscheidung. Ein zusätzliches Bereitschafts-Bool im
Computerkatalog würde diese Lücke nicht schließen.

## Verbindliche Arbeitsteilung und offene Abnahme


Bestehender Produkt-Lifecycle (Quellprüfung, keine neue Startarchitektur):
`apps/desktop/src/provisioning/DesktopComputerProvisioner.ts` führt lokal oder
per SSH CTOX-CLI-Aktionen für Start/Stop/Restart aus. CTOX hält seinen eigenen
Service; Workjets Node-Server ist derzeit nicht dessen dauerhafter Child-Host.
`apps/desktop/src/ctox/CtoxLocalDaemonLaunch.ts::resolveLaunch` löst die lokale
Instanz auf und ruft begrenzt `business-os desktop invite --format json` auf.
`CtoxSshManagedLaunch.ts::resolveLaunch` erzeugt das Remote-Invite und hält die
zugehörigen Signaling-Forwards. Beide liefern Shell-Startkonfiguration, bisher
keinen privaten Authority-Endpunkt. Der Anschluss muss daher aus dem laufenden
CTOX-Service kommen und im lokalen Workjet-Host bleiben; das vorhandene Browser-
Invite ist dafür nicht um einen entfernten Socketpfad zu erweitern.

- Native Sync-Aufgabe: produktive Host-Anbindung, bestätigte Konfiguration,
  Abnahme der aktuellen Mitgliedschaftsabfrage, Reconnect-Abnahme und unveränderte Autoritäts-
  und Signaling-Grenzen. `attach_worker` allein schreibt keine Aufnahme.
- Workjet-Aufgabe: produktive Aufrufer des vorhandenen Clients, persistierter
  Aufnahmevorgang und getrennte Darstellung von lokalem Listener, bestätigter
  Aufnahme, Verbindung, Kompatibilität und Ausführbarkeit. `hello` kann bereits
  als lokale Diagnose angebunden werden; es gibt noch keine freigegebene
  Operation, die allein einen produktiven SSH-/QR-Worker als einsatzbereit meldet.
- Gemeinsamer nächster Ablauf: Admin-Autorisierung und Schlüsselbesitz prüfen,
  dieselbe dauerhafte Aufnahme über Voter-IPC bestätigen, Worker mit seiner
  lokalen Identität und den drei geprüften Votern starten, aktuelle Mitgliedschaft
  abgleichen und anschließend echte Ausführung sowie Neustart testen.
- SSH verwendet die bestehende Installation/Authentifizierung. QR/Link verwendet
  die bestehende Invite-/Bindungsabwicklung. Für beide fehlen native Aufnahme-
  Integration und produktiver Signaling-Grant. Weder Browser-Credentials noch
  eine bloße Aufnahme von `workjet_executor` in eine Rollenliste schließen dies.

Quellbelege: `src/core/sync/src/ipc.rs`, `local_host.rs`, `native_execution.rs`,
`authority.rs`, `authority/client.rs` und `authority/node.rs`. Der historische
Beleg entsteht in `State::apply`; aktuelle Ausführungsberechtigung prüft
`AuthorityNode::local_validate` separat nach der linearisierbaren Quorum-Abfrage.
