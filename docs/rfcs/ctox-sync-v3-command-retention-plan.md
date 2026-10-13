# S4-Plan: abgeschlossene Befehle aus der Replikation nehmen

Status: Entwurf zur Freigabe, 13.10.2026. Heute nur Plan, keine Umsetzung.
Vertrag und Label: Sync-Wache über den Supervisor. Umsetzung: Architektur Rework.
Grundlage: [Sync-v3-RFC](ctox-sync-v3.md), §3.2, §3.6, §4 und S4.

## Befund und bestehender Pfad

Der Supervisor meldet für den frischen Workjet-Gast auf WELSCH 9.016 Zeilen in
`ctox_business_os__business_commands__v2`, insgesamt 28 MiB im Feld `data`.
Das ist die gemeldete SQLite-Datenmenge, keine unabhängig gemessene
DataChannel-Bytezahl. Anzahl terminaler bzw. noch aktiver Befehle und ihre
Altersverteilung sind noch unbekannt. Die ≤2-MB-Grenze gilt für den gesamten
Transfer bis sichtbare Daten, nicht nur diese Sammlung.

Auf der Plan-Basis `cf71d05d7` gibt es bereits eine Räumung:
[Frist und Batchgröße](../../src/core/business_os/browser_control.rs) sind
sieben Tage bzw. 256 Dokumente; [gc_settled_business_commands](../../src/core/business_os/rxdb_peer_browser.rs)
selektiert nach Status und `updated_at_ms` und ruft `bulk_remove_by_ids` auf.
Auch `blocked` gehört dort zur Auswahl. Daraus folgt noch nicht, warum die
gemeldeten 9.016 Zeilen vorhanden sind. Ob Frist, Zustandsauswahl, erneute
Projektion oder Tombstone-Transfer beitragen, wird getrennt geprüft.

Der [Command-Outbox-Pfad](../../src/core/business_os/rxdb_peer_commands.rs)
und die [Eigentumsentscheidung](../../HARNESS.md#business-os-command-architecture)
bleiben maßgeblich: kanonische Intents, Lifecycle, Effekte und Projektion-Outbox
gehören in den Kern; die RxDB-Sammlung ist eine Projektion. Die noch laufende
Migration darf nicht als fertige oder WAL-übergreifend atomare Speicherung
ausgegeben werden.

## Vorgeschlagener Vertrag

### Frist und Auswahl

Vorgeschlagener Default nach Freigabe: terminale Receipts bleiben **15 Minuten
ab dem dauerhaft gespeicherten terminalen Übergang** in der allgemeinen
Replikation. Der Zeitpunkt ist unveränderlich; erneute Projektion, Lesen,
Reconnect und Uhrzeit eines Wartungslaufs verlängern ihn nicht. Die Frist ist
ein Vorschlag, kein bereits gesetzter Runtime-Wert.

Nur dauerhaft bestätigte Endzustände werden ausgewählt: completed, endgültig
failed und cancelled einschließlich des kanonisierten Altwerts canceled.
`blocked`, retry_wait, wartende Freigaben, laufende/geleaste und unbekannte
Zustände bleiben erhalten. Wiederaufnehmbare Fehler sind nicht endgültig.
Ein Lease-/Effekt- oder Lifecycle-Widerspruch verhindert die Retirement-Aktion.

Lange Offline-Zeiten verlängern nicht den replizierten Verlauf. Ein Client
kann einen alten Receipt über seine genaue Command-ID und aktuelle
Eigentümer-Autorität wieder abrufen. Dafür darf er weder die ganze Historie
replizieren noch einen neuen Befehl mit anderer ID ausführen.

### Archiv und ein Schreiber

Intents, Idempotenz-Hash, Actor/Eigentum, Queue-/Effektbezug, Endzustand,
Resultat und Audit bleiben im kanonischen nativen Command-Bestand im Kern.
Kein Browser-Archiv und keine neue Schatten-/Projektionskette. Vor Retirement
muss die vollständige kanonische Ablage nachgewiesen sein; fehlt sie bei einem
Legacy-Record, wird zunächst über die vorhandene Migration nachgeführt.
Fehlende oder widersprüchliche Ablage ist ein sichtbarer Befund, keine Löschung.

Der vorhandene, generationengebundene Command-Projektionsschreiber verarbeitet
auch die Retirement-Outbox. Kein zweiter RxDatabase-Handle, keine separate
SQL-Räumung am Live-Store und kein zweiter Hintergrundschreiber.
Die Kerntransaktion speichert die Retirement-Absicht mit Lifecycle-/Projektionsversion;
die Veröffentlichung über das RxDB-Dokument-API erfolgt danach idempotent.
Absturz zwischen beiden Schritten wird aus derselben Outbox fortgesetzt.
Es gibt keine behauptete atomare Transaktion über mehrere SQLite-Dateien.

Die Retirement-Version bleibt dauerhaft bekannt. Outbox-Replay, Audit-Reparatur,
Peer-Neustart und ein alter Browser-Intent dürfen keinen alten terminalen
Vollbeleg in die allgemeine Replikation zurückschreiben oder erneut ausführen.
Ein genauer Receipt-Abruf benutzt einen autorisierten Demand-/Control-Pfad;
er erzeugt keinen dauerhaften replizierten Vollbeleg.

### Tombstones und Checkpoints

Ein vorhandener Client erhält zunächst eine policy-gefilterte Löschveröffentlichung.
Der replizierte Tombstone enthält nur die erforderliche Löschhülle und die
Attribution, die der native Filter für die korrekte Empfängergruppe benötigt.
Resultat, Prompt, Payload und Fehlerdetails bleiben im Kern. Ein bloßes
`_deleted:true` auf dem unveränderten großen Dokument wäre keine Lösung.

Ein frischer Vollsnapshot enthält die verbleibenden aktiven/rezenten Receipts,
keinen historischen Strom sämtlicher Löschhüllen. Auch 9.016 kompakte Tombstones
können das Startbudget verletzen. Die Implementierung muss deshalb die
Initial-Snapshot-Semantik und den vorhandenen Tombstone-Sweep/Checkpoint-Floor
gemeinsam prüfen, statt nur Zeilen unsichtbar zu markieren.

Physischer Tombstone-Abbau geschieht erst mit dauerhaft veröffentlichtem
Checkpoint-Floor. Ein älterer Resume-Checkpoint führt zu Reset und frischem
autorisiertem Snapshot. Vollsnapshot, währenddessen eintreffende Änderungen
und CaughtUp behalten ihre bestehende Reihenfolge; keine stille Vollständigkeitsbehauptung.
Eine Löschhülle wird nicht auf Basis fehlender aktueller Browserverbindungen
purged. Ein alter, lokal gepufferter Intent wird weiterhin gegen die kanonische
Idempotenz geprüft und kann den archivierten Effekt nicht wiederholen.

### Bestehende Clients und Datenklassen

Keine zusätzliche Pflichtspalte oder einseitige Schemahash-Änderung im
Mischbetrieb. Bestehende Clients können die normale Tombstone-Veröffentlichung
anwenden. Ein Client darf ein verschwundenes Receipt nicht als pending oder
erfolglose Ausführung interpretieren und automatisch neu ausführen.
Der genaue, autorisierte Receipt-Abruf und das Verhalten bei älteren
Checkpoints werden vor einer verkürzten Frist für diese Clientversion geprüft.

Ein dauerhaft replizierter Receipt bleibt ≤8 KB. Große Intents/Resultate sind
nach RFC §3.6 Demand-Daten; die replizierte Hülle enthält begrenzte Metadaten
und eine native Referenz. Das benötigt den S2-Profil-/Eigentumsvertrag und
kompatible Command-Bus-Verbraucher. **Eine kürzere Frist allein garantiert
weder ≤8 KB pro Dokument noch ≤2 MB bis Daten sichtbar.** Fehlt dieser
Verbraucherpfad, wird die Lücke mit Stufe/Owner ausgewiesen, nicht durch
abgeschnittene Ergebnisse oder einen HTTP-Datenpfad kaschiert.
Eventuelle UI-Befunde gehen an die UI-Owner; dieser Strang ändert keine UI.

## PR-Schnitt nach Freigabe

| Scheibe | Änderung | Nachweis |
|---|---|---|
| S4-A kanonisches Retirement | Alters-/Statusinventar; unveränderlicher terminaler Zeitpunkt, Archivprüfung, Retirement-Version und idempotente Outbox im bestehenden Command-Schreiber. | Grenze vor/auf/nach 15 Minuten; blocked/retry/aktive Leases bleiben; Legacy-Migration, fehlendes Archiv, Clock-Rücksprung und Crash vor/nach Veröffentlichung; kein erneuter Effekt. |
| S4-B Replikation und bestehende Clients | Kompakte policy-gefilterte Tombstones, frischer Snapshot ohne retired Historie, sichere Sweep-/Checkpoint-Floor-Anbindung und genauer Receipt-Abruf. | Online-Client, Offline-Resume vor/nach Floor, zwei Besitzer, entzogenes Konto, Neustart und Replay; Löschung kommt an, keine fremden Details, keine Wiederauferstehung. |
| S2/S4-C Datenklassen und Budget | Generierter Profil-/Schema-Vertrag, ≤8-KB-Hülle und Demand-Resultat; kompatible native/browserseitige Verbraucher gemeinsam. | Vollständiges Resultat auf Abruf, unveränderte Command-ID/Owner-Bindung, alte Clientversionen mit ausdrücklich geprüftem Verhalten; kein HTTP-Datenpfad. |
| S4-D Abschlussmessung | Gleiche S0-Hülle und unabhängige Byte-Oracle, frischer Gast mit deterministischer Last in der gemeldeten Größenordnung. | Vorher/Nachher-Tabelle einschließlich Einzelwerte, Bytes, Frist-/Retirement-Zahlen, Versions-/Lane-Beleg; keine Budgetbehauptung aus SQLite-Summen. |

Keine neue Prioritätssonderregel zusätzlich zu #650. Jede Code-Scheibe erhält
ihr Stage-Label über die Wache. Benötigte S2/S5-Vertragsanteile werden vor dem
Code im Scope benannt, nicht als unregistrierte S4-Nebenwirkung eingeführt.

## Messplan und Freigabekriterien

Fixture: 9.016 deterministische Command-Records, ungefähr 28 MiB vor Retirement,
mit deklarierten terminalen, rezenten und aktiven Anteilen sowie großen Resultaten.
Keine Kopie von Kundenrecords, keine erfundenen Grants. Zusätzlich der feste
S0-Scale-Fall mit 6.000 Befehlen und den bisherigen Leads/Tasks/Chats.
Die alte und neue Version laufen mit derselben Fixture und tatsächlichen
RTCPeerConnections/DataChannels auf der zugelassenen Lane im isolierten Tenant.

Je Quelle und Relay-Fall 0/300/600 ms: mindestens drei frische Browserkontexte,
derselbe Reset und feste Wartezeit. Erfasst werden Navigation bis zu tatsächlich
sichtbaren Daten innerhalb der S0-Hülle, individuelle Werte und Median, App-ready,
interaktive Abfrage-p95, Schreib-ACK-p95, Heap/IndexedDB und empfangene Wire-Bytes
bis exakt diesem Sichtbarkeitspunkt. Die geprüfte unabhängige Byte-Oracle gilt
über alle Spuren und Sammlungen; reine JSON-/SQLite-Größen genügen nicht.
20 tatsächliche Writes und 20 Command-Receipts prüfen, dass Retirement den
Rückweg nicht beschädigt.

Gate: §4-Budgets bleiben unverändert, insbesondere ≤2 MB bis Daten sichtbar
und kein repliziertes Dokument >8 KB. Die gesetzten S5-Ziele sichtbar ≤10
RTT-Äquivalente bei 600 ms und ACK-p95 ≤3 RTT bleiben bestehen; sie werden
durch die S4-Scheibe nicht als erreicht ausgegeben. Bounded Batches und kurze
Write-Transaktionen werden unter Last gemessen; keine lang gehaltene
IMMEDIATE-Transaktion um Inventar, Archivscan oder Netzwerk-Await.

Installed-Abnahme erfolgt erst nach Supervisor-Deployment mit demselben
freigegebenen Verfahren. Die heutige #650-Messung über den WELSCH-Tunnel
bleibt separat: kein S4-Fault-/Räumversuch auf WELSCH, kein THESEN-Kontakt.
Auswertung und Beleg enthalten Erfolge und Fehler, Versionsstände, Gastalter
und noch unbekannte Messgrößen. Heute wird lediglich dieser Plan reviewbar
bereitgestellt.
