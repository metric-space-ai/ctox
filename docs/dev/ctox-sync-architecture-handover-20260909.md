# CTOX-Sync-Architektur: Handover vom 9. September 2026

## Nachprüfung nach der Übergabe — 9. September 2026

Weitere Nachprüfung: Commit `1c21f8bd96d81e4ba5148bef379e34ab7939794a` ist nach
Wiederherstellung der SSH-Verbindung in PR87 gepusht; die zuvor dokumentierte
lokale Push-Sperre ist damit aufgehoben. Die neue Profiler-Diagnostik hält sowohl
stdout als auch stderr mit jeweils höchstens 16.384 Zeichen plus Truncation-Flag
fest. Zuvor wurde stdout verworfen, obwohl Exit-255-Aufnahmen ohne stderr auftraten.
Dies beweist nicht deren Ursache. Exit-/Signal-/Dateigrößen-/Sample-Guards bleiben
unverändert. Drei gezielte Tests mit simuliertem Recorder bestehen (begrenzte
Aufnahme, Permission-Fehler, begrenzte Ausgabe auf beiden Kanälen). Kein echtes
Linux-perf oder vollständiger Browser-/Native-Lauf wurde dadurch ersetzt.

Nachtrag zur Recovery-Prüfung: Die Fixture las die verknüpfte Queue-Task per ID,
prüfte Eindeutigkeit danach aber über die allgemeine `find()`-Listenmitgliedschaft.
Der Query-Loader darf bereits geladene Fenster zunächst stale liefern. Die drei
Cardinality-Prüfungen für Reload, Burst und Restart verwenden nun eine serverseitige
`command_id`-Selektion und `requireRevision: command-link:<id>`. Dieser vorhandene
Caller-Token erzwingt einmalig eine abgeschlossene Aktualisierung, ist ausdrücklich
keine Serverrevision. Anzahl, Deadline, Wiederherstellung ohne Reparatur und
Wiederholungsverbot bleiben unverändert. Syntax-/Whitespace-Prüfung und bestehender
In-Memory-requireRevision-Test bestehen. Echte Browser-/Native-Wiederherstellung
bleibt bis zum neuen CI-Lauf offen; dies ist keine Behebung oder Freigabe der
allgemeinen veralteten Listenansicht im Produkt. Der Download der älteren
Recovery-Artefakte scheiterte zweimal am Azure-Blob-Netzwerk-Timeout; die zugehörigen
Job-Logs wurden gelesen, fehlende Artefakte nicht als erfolgreiche Prüfung gewertet.

Der damals noch offene Post-Merge-Lauf
[34338432749](https://github.com/metric-space-ai/ctox/actions/runs/34338432749)
ist **fehlgeschlagen**. Linux/macOS Native Sync, Chromium und der separate
Profiler-Job bestehen; die vollständige Host-Abnahme scheitert an Kontext-Command,
Command-Budget und abschließender Profilaufnahme. Die grüne Kandidaten-Abnahme
weiter unten bleibt historische Evidenz und ersetzt diese fehlende Freigabe nicht.
Die gesicherten `command-stages.json` enthalten 30 vollständige Messungen ohne
Stage-Issues: Gesamt-p50 301,5 ms, p95 364,7 ms. Die separate `command-budget.json`
ist leer, nicht ein grüner Budget-Beleg. Der eingebettete perf-Aufruf endet mit
Code 255, leerem stderr und `reason=perf-record-failed`; Ursache noch unbewiesen.

Spätere main-Läufe 34342505863 und 34346965129 sind vollständig erfolgreich.
Der folgende Lauf 34349242187 ist wieder rot: Command-Recovery nach Native-Ausfall
meldet null statt einer Queue-Task; die Profilaufnahme scheitert ebenfalls.
Damit ist eine dauerhaft robuste Abnahme nicht durch einen einzelnen grünen Lauf
belegt. Der zuletzt geprüfte Lauf 34351747131 zu `3b8ff841a` hat einen roten macOS-
Credential-Test; Full Host läuft bei dieser Nachprüfung noch. Keine dieser
Beobachtungen allein beweist, welche Codeänderung oder Umgebungsbedingung die
Abweichungen verursacht.

Der macOS-Test `wrong_source_pin_or_instance_never_requests_credentials_over_real_webrtc`
scheitert an `public proof traversed WebRTC`. Im Test wurde vorher jedes erste
Event aus dem gemeinsamen Peer-Fehlerstream als erwartete Ablehnung behandelt.
Die neue Testkorrektur wartet innerhalb derselben 35-Sekunden-Deadline ausdrücklich
auf `local_session_credentials_unavailable`. Diese Zuordnung entspricht der
Fehlerveröffentlichung in `attach_local_session`; die Assertions für echte
Proof-Übertragung, null Credentials-/Signatur-Callbacks und fehlende Admission
bleiben unverändert. Das ist eine zu verifizierende Korrektur der Testbeobachtung,
noch kein Nachweis einer behobenen Produktionsursache. Die vier betroffenen
Tokio-Test-Runtimes verwenden jeweils zwei statt vier Worker.

Der inzwischen gelesene Remote-main ist `b1ccb1e64dd491c2097463f4e6ba309c3ca81161`.
Er enthält zwischenzeitliche Korrekturen anderer Aufgaben, unter anderem an
Crew-/App-Tests. Diese Änderungen nicht aus dem alten Checkout überschreiben.
Der kanonische lokale CTOX-Checkout ist divergent und enthält fremde Änderungen;
die Nachprüfung verändert ihn nicht. Rohbelege liegen dauerhaft unter
`/Users/michaelwelsch/.codex/notes/ctox-sync/evidence/postmerge-34338432749/`.

### Aktualisierte lokale Ressourcengrenzen

Die neuen AGENTS-Regeln ersetzen die unten historisch genannte 2,5-GiB-Grenze:
Vor schweren Jobs hostweiten Status prüfen und ausschließlich über
`greppy bash-smart -- /usr/bin/python3 /Users/michaelwelsch/.codex/bin/dev-heavy-run.py --owner <thread-id> --project ctox --task <task> -- <command>`
starten. Erforderlich sind mindestens 20 GiB frei auf System **und** tmp, höchstens
8 GiB Swap, Last höchstens CPU-Anzahl, ein schwerer Job hostweit und höchstens zwei
Worker. Bei Nachprüfung: System 27,42 GiB frei, tmp 5,47 GiB, Swap 4,48 GiB,
Load 15,03 bei 10 CPUs, anderer Cargo-/rustc-Prozess aktiv. Daher keine lokale
native Kompilierung oder schwere Testausführung; keinen fremden Prozess stoppen.
Tmp ist nach spätestens vier Tagen weg. Source und PR während jeder Arbeitssitzung
sichern; keine ausschließlich dort liegenden lokalen Commits übergeben.

## Auftrag und Grenzen

Der Nutzer hat beauftragt, den sicher belegten Umfang nach `main` zu mergen und
anschließend dieses Handover zu erstellen. Die sechsstufige Offensive ist nicht
abgeschlossen. Ein Merge ist keine Freigabe für Kunden-Upgrades, Workjet-Releases
oder Harness-Failover. Diese Zustände werden unten getrennt ausgewiesen.

Keine Kundeninstanz wurde für diese Übergabe aktualisiert, neu gestartet oder
bereinigt. Insbesondere führt der Betreiber das Upgrade von `thesen.ctox.dev`
aus. Office-Implementierungen liegen bei der anderen Aufgabe. Fremde Checkouts,
Arbeitsdaten, Browserprofile und Build-Artefakte wurden nicht bereinigt.

## Git und Merge-Entscheidung

- Repository: `metric-space-ai/ctox`.
- Quell-PR: https://github.com/metric-space-ai/ctox/pull/69
- Branch: `codex/native-transport-parity`.
- Geprüfter Quellstand: `80f7150bfee415e5fc5655a08d9e8fbde223a596`.
- Bei Prüfung aktuelles main: `5c6cc0214d6a24255c495169b59c399a6873c30f`.
- GitHub-Merge-Kandidat: `edf7d4b46eccfae262a5ff3da1db2226c80611fb`,
  Tree `999f618ad0bc4f97255a6945867272f8721351bf`.
- Merge-Ergebnis: PR69 am **2026-09-09 10:06:29 UTC** nach main gemergt,
  Merge-Commit **`00a574654477adafac32a89bdbe319365b5d6068`**.
- Während der Übergabe kamen auf main `fa91e485e88e9506dabe6b7ce52e3ba1672ade71`
  (zwei Person-Research-Dateien) und `d40c18c3c75aa25e53d0ae2396950ad2919e7337`
  (ausschließlich Timeout-Fehlertext/Kommentar in app.js) hinzu. Beide bleiben
  erhalten; der tatsächliche Merge hat Eltern `d40c18c3` und `80f7150b`.
  Der Delta zum gemessenen Kandidaten betrifft genau diese drei Dateien.
  Die vollständige Messreihe lief auf dem Kandidaten mit main `5c6cc0214`,
  nicht erneut auf dieser späteren Kombination. Post-Merge-Native-Sync:
  https://github.com/metric-space-ai/ctox/actions/runs/34338432749
  (bei Übergabe noch nicht abgeschlossen). Kein Kunden-Release abgeleitet.
- Merge erfolgte über die normale GitHub-PR-Funktion mit festgelegtem Head,
  ohne Admin-Override, Force-Push oder Änderung von Prüfstatus/Guards.

Der PR enthält zusammenhängende Transport-, Command-, Shell- und native
Kernänderungen. Eine Herauslösung früherer Commits würde insbesondere spätere
Admission-/Identitätskorrekturen verlieren und wäre ein neu zu prüfender Stand.
Die noch fehlenden Produktfunktionen werden durch vorhandene Verträge oder
Prüf-Fixtures nicht als implementiert ausgewiesen.

## Implementierungsumfang

### Replikation und Commands

Browser- und native WebRTC-Eingangspfade trennen Frame-Eingang von begrenzter
RPC-Ausführung. Handshakes behalten Kapazität unter Daten-Backpressure. Query-
Slots, Requests und Verbindungen sind an die aktuelle Verbindungsgeneration und
ihren Lebenszyklus gebunden. Eingehende RPCs prüfen die Peer-Berechtigung erneut;
fehlende oder beschädigte Authority-Daten werden nicht als Erlaubnis behandelt.

Native Query-Pages verwenden denselben WebRTC-Request-/Chunk-/ACK-/Cancel-Pfad,
mit Grenzen für Seitengröße, Dekompression und Parallelität. Es wurde kein
Browser-Business-Data-HTTP-Fallback eingeführt. Core-Reader und SQLite-Reader-Caches
werden wiederverwendet und begrenzt.

Command-Intake bleibt geordnet und begrenzt. Atomare Domain-Receipts ermöglichen,
bereits bestätigte fachliche Wirkungen und deren Ergebnisse nach einem Ausfall
wieder zu projizieren, ohne die Mutation erneut auszuführen. Dies ist keine
allgemeine Exactly-once-Garantie für externe Aktionen mit unbekanntem Ausgang.
Projekt-/Chat-/Worker-Beziehungen und deren serverseitige Sichtbarkeitsprüfungen
sind Teil des nativen Business-OS-Vertrags.

Einstiegspunkte: `src/core/business_os/rxdb_peer.rs`, `rxdb_peer_intake.rs`,
`rxdb_peer_domain_recovery.rs`, `command_plane.rs`, `domain_effect.rs`,
`project_chats/`; `src/core/rxdb/src/plugins/replication_webrtc/` und
`storage/sqlite/`; `src/apps/business-os/rxdb/src/inbound-request-queue.mjs`
und `demand-loading-transport.mjs`.

Verträge: [Domain-Recovery](../domain-effect-recovery.md),
[Workjet-Chat-Consumer](../workjet-project-chats-consumer.md).

### Shell und unmittelbare Chat-Präsentation

Lokale Chat-Fenster und Kontext-Prompts warten beim Öffnen nicht mehr auf eine
vollständige History-/Storage-Hydration. Ein verspätetes Command-Ergebnis schließt
keine neuere Eingabe oder ein inzwischen anderes Kontextmenü. Collection-Leases
bleiben an die aktive Runtime gebunden; ausgemusterte Crew-Views geben Observer
und Retry-Arbeit frei. Veraltete HTTP-Readiness-Overrides wurden entfernt.

Der integrierte Shell-Stand ist `20260909-sync-main-integration-v367`. Die
belegten UI-Szenarien schließen die Änderungen des damaligen `main` ein. Das
beweist keine korrekte aktivierte Shell auf Welsch oder Thesen. Slots dürfen
weiter nur aus `main` gebaut werden. Native-Release, Shell-Slot und tatsächlich
geladene Runtime müssen beim späteren Upgrade gemeinsam nachgewiesen werden.

### Nativer Kern und Session-Grundlagen

`LocalIpcHost` plus `IpcService` ersetzt den nur an Authority gebundenen lokalen
Host. Authority bleibt ein Adapter am selben privaten Unix-Socket mit unveränderter
Framing-/Zugriffskontrolle. Shutdown beendet und leert die eigenen Connection-
Futures. Ein echter Zwei-Client-Unix-Socket-Test prüft Betrieb ohne Raft-Node.
Windows erhält hierdurch keinen neuen produktiven Listener.

Checkpoint-Schutz und Übernahme sind über den generierten Authority-IPC-Vertrag
und den echten gepinnten Workjet-Client erreichbar. Quorum, Generation, bestätigte
Datenkopien und ungeklärte Effekte bleiben verbindliche Guards. Ein reiner
Koordinations-Voter zählt nicht als geschützte Datenkopie.

Die gemeinsame BusinessData-Fixture generiert Rust-, TypeScript- und Effect-
Verträge für Session/Query/Watch/Command. Sie ist noch kein operativer Dienst.
SQLite kann Collection-Counter und gebatchte Dokumente in einer gemeinsamen
Lesetransaktion erfassen; Abbruch oder Lesefehler liefern kein Snapshot-Ende.
Der Counter ist ausdrücklich kein dauerhafter Resume-Cursor.

Native Quellen attestieren Challenge, Instanz, aktuellen optionalen Principal und
die aktuelle DTLS-Kanalbindung mit dem vorhandenen Sync-Ed25519-Schlüssel.
`NativeSessionTarget` verlangt einen unabhängig bestätigten Instanz-/Key-Pin.
Der Kern prüft diesen Beleg vor dem Abruf eines Bearers oder einer Credentials-
Signatur. Eine falsche Quelle/Instanz löst keine Credentials-Callbacks aus.
Öffentliche Identity-RPCs autorisieren weder Collections noch Authority-Commands.

Hauptvertrag: [Native BusinessData](../ctox-native-business-data-contract.md).
Fixture: `src/core/rxdb/tests/fixtures/ctox_business_data_contract.json`.
Generator: `src/core/sync/tools/generate-contracts.mjs --business-data --check`.
BusinessData-Hash: `7ba81d4d067c3d68c0952badca2486318a3e1276bd24fb5d41f40c822fbc98b2`.
Authority-Hash: `ec12f0b1360bc7e6eef4187f2a8cd13787c1253f7031dcd5d3bd9c885503bae2`.
Workjet-IPC-Consumer-Pin: `74c0fccf8515a0f4f8826321e507d6847aa0de64`.

## Abnahme und Performance

Aktueller [Native-Sync-Lauf 34334156791](https://github.com/metric-space-ai/ctox/actions/runs/34334156791):
Alle fünf Jobs einschließlich vollständiger Host-Abnahme sind erfolgreich.
Die Belege nennen exakt den oben angegebenen Merge-Kandidaten `edf7d4b46`:
Binary-SHA256 `5b52fd68af61ee5e231f89f770dcf4716021e8f3a9ba33c0a9f8f363a9fe4686`.
Aktuelle warme Browser/WebRTC/native Commands: n=30, p50 **289 ms**, p95
**365,1 ms**; Median-Grenze 300 ms eingehalten, nur 11 ms Reserve.
Kritische Collections nach Reload: n=30, p95 **3.104,690 ms**, Grenze 5.000 ms,
keine Diagnose-Issues. Vier-Prozess-Abnahme erfolgreich; weiterhin
`codingHarnessExecuted=false`. Die Source-/Binary-/Messbelege liegen dauerhaft
unter `evidence/full-proof-34334156791/`. Keine kontrollierte Speedup-Behauptung.

Im inspizierten Linux-Protokoll bestehen der neue Zwei-Client-Shutdown-Test und
die echte WebRTC-Prüfung mit null Credentials-Callbacks bei falschem Pin/Instanz.
RxDB: 423 Tests plus Integrationsgruppen 31/1/1/4 bestanden.
SQLite-Einzelmessung: 30 Snapshots, je 1.000 Dokumente mit 1.024 Content-Bytes,
Batch 100: p50 23,944 ms, p95 28,337 ms (nearest-rank). Dies enthält weder
Transport noch UI und ist kein Nachweis eines WAN-/Produkt-Performance-Gewinns.

Vollständig bestandener vorheriger Integrationslauf zu `21a01d0d9`:
[34331051775](https://github.com/metric-space-ai/ctox/actions/runs/34331051775).

- Warme Browser/WebRTC/native Commands: n=30, p50 273 ms, p95 334,2 ms;
  verbindliche p50-Grenze 300 ms eingehalten.
- Fünf kritische Collections nach Reload mit behaltenem Browserprofil:
  n=30, p95 2.987,821 ms, Grenze 5.000 ms, keine Diagnose-Issues.
- Vier echte native Prozesse: drei Voter, ein Worker; Membership, Reconnect,
  Neustart und Widerruf bestanden. Topologie localhost, Datensatz ein Control-Job.
- `codingHarnessExecuted=false`: kein Codex-/Claude-Resume, keine vollständige
  Arbeitsverzeichnis-/Attachment-Portabilität, kein Produkt-Failover-SLA.

Zusätzlich erhaltene Browser-Fixtures des integrierten Shell-Stands: 115 Chat-
Szenarien, drei CTOX-Geometriebreiten und drei Kontextmenü-Modi bestanden.
Sie verwenden Fixture-Daten/Auth-Grenzen und ersetzen keine Kundeninstanz-Abnahme.
Einzelne Paint-Zeiten sind keine Command-p50/p95-Messung.

## Bestehende rote Baseline und Release-Blocker

Die Standard-CI ist nicht vollständig grün. Verglichen wurden der PR-Lauf
[34334156822](https://github.com/metric-space-ai/ctox/actions/runs/34334156822)
und der main-Lauf [34331547026](https://github.com/metric-space-ai/ctox/actions/runs/34331547026)
zum Stand `5c6cc0214`:

1. Linux-CLI-Job scheitert in beiden Läufen am unveränderten App-Platform-Guard:
   `new module-local contextmenu handler: modules/explorer/index.js`.
2. Legacy-Desktop-Linux-Job scheitert in beiden Läufen vor den E2E-Tests an
   `npm audit`: js-yaml 4.0.0–4.3.1, GHSA-2883-xcg3-v3hh (high).
   Der Audit nennt 4.3.2 als Fix; kein ungeprüftes `audit fix --force` ausführen.
3. Auch die entsprechenden macOS-/Windows-Jobs sind rot; die detaillierte
   Baseline-Gegenprüfung oben bezieht sich ausdrücklich auf Linux.
4. Crew-Liveness-Lauf 34334156802 meldet zwei Outbound-Fixture-Fehler:
   `outbound_adapter_reconciliation_projects_typed_result_without_secrets`
   (`adapter writeback`) und
   `outbound_adapter_reconciliation_rejects_invalid_batch_before_any_write`
   (`first source`). Dieselben Fehler sind in 34331051702 und im unabhängigen
   Branch-Lauf 34331145758/e1bd14cc belegt. Dieser unabhängige Branch enthält
   weder die Sync-Offensive noch Änderungen an store_outbound_commands.rs oder
   store.rs. Die Tests erwarten RxDB-Daten in frisch angelegten Test-Roots;
   Writer/Reader behandeln fehlende Collection-Tabellen unverändert als abwesend.
   Keine neue reine-main-Ausführung dieses Filters durchgeführt; die gemeinsame
   Fehlerlage ist damit belegt, ihre endgültige Behebung bleibt offen.
   Der aktuelle Crew-Lauf ist abgeschlossen: ausschließlich die Admission-
   Regressionsstufe scheitert, Clippy, native RxDB- und Browser-Wire/JS-Stufen
   bestehen. Der Android-Donor-Job war beim Merge noch nicht abgeschlossen;
   daraus folgt keine Mobile-Produktfreigabe.

Der PR verändert weder Explorer, Freeze-Guard noch den Legacy-Desktop-Baum.
Die Befunde werden nicht ausgeblendet, Guards nicht abgeschwächt, Jobs nicht
als erfolgreich ummarkiert. Laut [Produktmatrix](../product-matrix.md) ist der
Electron-Baum `src/apps/business-os-desktop` ein Migrationsspender, kein Workjet-
Releaseziel. Echte Workjet-Desktop-/Mobile-Prüfungen bleiben erforderlich.

## Offener Umfang nach den sechs Etappen

| Etappe | Erreicht | Noch erforderlich |
| --- | --- | --- |
| 1: Ist-Stand/Ablösung | Verträge, Defektliste, reproduzierbare Host-/Browser-Fixtures | Vollständige aktuelle Ablösematrix aller drei Repositories; historische Statusabschnitte nachführen |
| 2: Kern/Authority | Nativer Lifecycle, Mehrheits-/Generationsprüfungen, privater IPC-Host | Produktives Onboarding und Workjet-Datendienst; Prozess-/Gateway-/Tool-Wirkungsgrenzen; Windows-Host |
| 3: Portabilität/Failover | Authority-Handoff, Datenkopien-Guards, Snapshot-Baustein | Journal, Dateien, Anhänge, echte Harness-Checkpoints, Codex/Claude Export/Import/Resume; WAN/Partitionen und externe Effekte |
| 4: Shell/Instanz-Lifecycle | Chat-/Collection-/Observer-Lifecycle und kanonischer Shell-Build integriert | RuntimeManifest-/Release-Nachweis auf Web/Desktop/Mobile; Suspend/Resume; Kunden-Upgrade |
| 5: Altpfade entfernen | Einzelne ersetzte Host-/Admission-/Readiness-Pfade entfernt | Operative BusinessData-Anbindung; danach alte Guest-Datenpfade/Status-Reparaturen entfernen |
| 6: Migration/Cutover | Teilmigrationen und Domain-Receipt-Recovery | Migrationsbilanz realer Bestände, Restore-Probe, bestätigte aktive Sessions und koordinierter Client-/Native-Wechsel |

Thesens Incident-Abnahme bleibt ein eigener offener Produktnachweis: frischer
Reload, alle 21 Collections vollständig unter 60 s, aktuelle Recherchefelder,
zugestellte Tombstones, bestätigte Schreibvorgänge, keine Transfer-Stalls und
keine systematische CPU-Projektionsschleife. Lokale CI-Zeiten beweisen das nicht.
Historisch beschädigte Kundendaten nicht pauschal löschen: konkrete Klassifizierung,
gesicherte Originale und überprüfte Wiederherstellung sind erforderlich.

## Nächster konkreter Integrationsschritt

1. Native BusinessData Open/Close/Events implementieren: Connection-/Account-
   gebundene Handles und Generationen, begrenzte Frames/Queues, Abbruch und
   Antwortkorrelation während eines ausstehenden Open. Der serielle Authority-
   Request-Loop darf nicht durch einen Credentials-Rückruf blockieren.
2. Main-Prozess bleibt Secret-Owner. Unabhängigen Target-Pin auflösen, Account-
   Invalidator zuerst registrieren, lokalen Epoch synchron erfassen, Credentials
   erst nach Quellenprüfung und erneutem Epoch-Check bereitstellen. Erst der
   bestätigte native Principal darf die Session als aktuell bestätigen.
3. Watch/Resume: Quelle/Epoch/Schema/Query/Auth binden, Änderungsverlauf vorhalten,
   bei Lücke/Overflow/fehlender Historie explizit zurücksetzen. `snapshotEnd`
   ist noch kein `caughtUp`.
4. Echte Desktop-/Mobile-Stories für SSH und QR/Link einschließlich Unterbrechung,
   Kontowechsel und Suspend prüfen. Danach ersetzte Guest-Datenzugriffe löschen
   und Wieder-Einführungs-Guards ergänzen.
5. Getrennte Messreihen für Klick-zu-Chat, bestätigte Commands, Boot, große
   Sessions, WAN und Failover erfassen. Keine gemeinsamen Durchschnittswerte.

### Workjet-Koordination: gemeldet, noch nicht hier abgenommen

Die autorisierte Aufgabe `01a08237-a9c4-77f3-9f20-f0fb3901a76d` meldete während
der Übergabe Workjet PR50/`6aa6472388b5ed53b9633f33c4593eb75808c2dd`:
`CtoxNativeIdentityResolver.resolve(targetId)` liefert Target, Instanz, Key und
Session-Epoch über dieselbe AccountLifecycle-Instanz. Local/SSH verwenden die
bestehende vertrauensgeprüfte Discovery bzw. knownHosts und ausschließlich
`ctox sync identity --root ...`; kein implizites `init`, kein Secret-Abruf.
Managed/QR ohne echten Enrollment-Pin und Windows bleiben fail-closed.
Laut Meldung ist CI 34336809108 offen. Dies ist keine hier unabhängig bestätigte
Laufzeit-/UI-Abnahme. NativeSessionTarget, Principal, Credentials-Callback und
Open/Close/Events sind noch zu verbinden.

`ctox sync identity --root <existing-root>` liest denselben vorhandenen Key aus
SecretStore-Scope `ctox-sync-host`, Name `identity-pkcs8`; es erzeugt/rotiert ihn
nicht. `ctox sync init` gehört zu explizitem Erst-Setup. `ctox sync run` startet
einen konfigurierten Voter/Worker-Control-Host, keinen allgemeinen Desktop-Datendienst.

## Belege, Arbeitsumgebung und Aufräumstatus

Durabler Checkout: `/Users/michaelwelsch/Documents/ctox-sync-main.2ITnXD`.
Durable Belege: `/Users/michaelwelsch/.codex/notes/ctox-sync/`, insbesondere
`evidence/full-proof-34331051775/`, `evidence/ipc-host-80f-linux.log`,
`native-ipc-service-host-20260909.md`, `native-precredential-proof-20260909.md`
und `native-business-data-ipc-handoff-20260909.md`.

Builds/Tests/Lints über `greppy bash-smart -- ...`; Navigation und Dateien über
Greppy. Bei fehlender Graph-Datenbank die bereits ausgewiesenen `greppy rg/read-file`-
Alternativen verwenden, keinen Vollindex auf dem knappen Datenträger aufbauen.
`docs/architecture.md`, auf das AGENTS verweist, fehlt im geprüften Checkout;
aktuelle Verträge stehen in den oben verlinkten Docs. Ältere Statusabschnitte in
`ctox-sync-core-offensive.md` und Transport-Parity-Notizen sind historische Belege;
dieses Handover nennt den zuletzt tatsächlich überprüften Stand.

Vor lokalem Build `/Volumes/tmp` prüfen. Zuletzt 1,2 GiB frei, unter dem vorhandenen
2,5-GiB-Native-Build-Guard. Deshalb native Builds/Tests auf CI; kein Fallback auf
Systemdisk und keine Löschung fremder Artefakte. Neue Build-/Test-Daten unter
`/Volumes/tmp/dev-artifacts/ctox/<task>/`. Source, Handover und Belege dauerhaft
halten. Kein Worktree oder fremdes Artefakt wurde für diese Übergabe entfernt.
