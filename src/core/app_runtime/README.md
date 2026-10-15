# Native App-Laufzeit (Backend)

Das Handler-Kit in `handler-kit/` ist unabhängig nutzbar (Slice A).
Dieses Rust-Paket und das eingebettete `worker.mjs` bilden den Prozess-Supervisor
für Slice B. Es benötigt keinen JS-Build und lädt zur Laufzeit keine Pakete.

## Schnittstelle zum Core

`RuntimePool::new(node_path, private_runtime_root, Arc<dyn RuntimeHost>)`
erhält absolute Pfade aus der Core-Konfiguration. Es setzt keine
Produktions-Umgebungsvariablen. Der Node-Pfad muss auf die bereits ausgelieferte,
verifizierte Laufzeit zeigen; es gibt keinen Download oder PATH-Fallback.

`register(AppBundle::new(manifest, sources, approved_grants)?)` erhält
Manifest/Quellen aus dem verifizierten installierten Release. Alle Quellen werden
gegen `manifest.files` (SHA-256) geprüft. Die Signaturprüfung/Release-Auswahl und
die Erteilung von Grants bleiben Aufgaben des Core. Ein Register-Aufruf ist
kein öffentliches App- oder Browserrecht. Eine neue Version beendet die alte
Prozessgeneration und verwirft ihren Modulzustand.

`invoke(app_id, actor, command_type, input)` wählt ausschließlich einen
deklarierten Handler. `migrate(app_id, actor, collection, docs)` erzeugt
Migrationsvorschläge, keinen Commit. `unregister(app_id)` beendet den Prozess;
bereits wartende Aufrufe der abgelösten Version werden abgewiesen.

Der Core muss `RuntimeHost` implementieren:
- `authorize(app, actor, operation)`: verbindliche Actor-/Policy-Prüfung vor
  jedem Handler/Migrationsaufruf und jedem ctx-Callback, einschließlich aktuell
  erforderlicher Bestätigungen und widerrufener Rechte. Actor stammt aus der
  verifizierten Core-Session, nie aus App-Code.
- `exported(owner, collection, operation)`: aktuelle Eigentümerschaft und
  Exportfreigabe; wird bei jedem Fremdzugriff zusätzlich zum Manifest geprüft.
- `capability_available(name)`: registrierte generische native Fähigkeit.
- `invoke(app, actor, method, args)`: Store-/Befehls-/Capability-Adapter.
  `read` bekommt `[collection, mango]` und liefert die gefilterte Dokumentliste;
  `write` bekommt `[collection, op]` und muss den bestehenden Core-Schreibweg
  benutzen, einschließlich Schema, Basisrevision und Commit-Ergebnis.
  Keine Spiegel-Datenbank und keine JS-Schreibberechtigung auf Core-Dateien.
  Capability-Aufrufe sind `["namespace.method", args[]]`.
  Fehler erhalten einen V4-Code als Präfix, z.B. `conflict: source revision`.

Die konkrete Store-/Policy-Anbindung an den Core und dessen
App-Publish-/Command-Lifecycle ist noch offen. Das Paket ist noch kein aktivierter
Ersatz des vorhandenen Business-OS-Ausführungswegs.

## Prozessgrenze

Ein Prozess je App, lazy gestartet und zwischen Aufrufen wiederverwendet.
Ein Core-Maintenance-Thread überwacht Prozesse und beendet sie nach zehn Minuten
Leerlauf. Abgestürzte Prozesse werden ersetzt; ein fehlgeschlagener Start führt
zu `unavailable`, ohne endlose Startversuche. Fehlgeschlagene Handler werden
niemals automatisch erneut ausgeführt.

Node startet mit leerer Umgebung, `--permission`, **keinen** allow-Flags,
`--max-old-space-size=256` und einem eingebetteten ESM-Einstieg per `--eval`.
Die Runtime erfordert Node >=25 und prüft die tatsächlich entzogenen Rechte
(Netzwerk, Dateisystem, Prozesse, Worker, Addons) vor dem Ready-Signal.
Node 22/24 erfüllen den benötigten Netzwerkentzug nicht; sie werden abgewiesen.
Siehe [Node-Permissions](https://nodejs.org/download/release/v25.0.0/docs/api/permissions.html).
Node selbst bezeichnet die Permissions nicht als Schutz vor bösartigem Code;
nur vom Core verifizierte App-Releases dürfen registriert werden.

Der Core öffnet einen privaten LocalTransport-Kanal vor dem Start und vererbt
dessen Handles als stdin/stdout. Der App-Prozess erhält dadurch kein Recht,
selbst Sockets zu öffnen. Unter Unix werden zusätzliche geerbte Deskriptoren
beim Exec geschlossen; die Eltern-Handles bleiben unverändert. Es gibt keinen
TCP-/HTTP-Fallback. App-Module werden
aus den geprüften In-Memory-Quellen geladen; relative Importe innerhalb des
Bundles sind möglich, Node-Builtins und dynamische Importe nicht.
Native Fähigkeiten stehen ausschließlich über ctx zur Verfügung.

Ein Core-Watchdog misst die akkumulierte CPU-Zeit ausschließlich des eigenen
Node-Kindprozesses (OS-Zähler, sysinfo, 25-ms-Abtastung) und beendet ihn bei
30 Sekunden CPU je Aufruf. Wartende Core-I/O wird nicht als App-CPU verbucht.
Zusätzlich gilt eine obere Wartefrist von zehn Minuten. Entfernen/Ablösen der App
setzt ein Abbruchsignal; ein laufender Node-Aufruf wird beendet und nicht
automatisch wiederholt. Der Adapter muss lange Core-Arbeit als dauerhaft
gespeicherten Befehl einreihen und darf den Pool nicht synchron zurückrufen.
Der Watchdog kann den App-Prozess stoppen, aber keinen beliebigen synchronen
Core-Callback abbrechen. Die CPU-Zähler-API ist in der
[sysinfo-Dokumentation](https://docs.rs/sysinfo/0.38.4/sysinfo/struct.Process.html#method.accumulated_cpu_time)
beschrieben.

Weitere Grenzen: 256 MiB V8-Heap (keine RSS-Garantie), 4 MiB App-Quelltext,
1 MiB Handler-Eingabe/-Ergebnis, 8 MiB IPC-Rahmen. Migrationen liefern
Vorschläge mit Basisrevision; die Übernahme in die nächste Generation erfolgt
separat im Core-Writer und benötigt dort erneut einen Versionscheck.

## Prüfung

Nur über die Build-Lane; Beispiel mit bereits bereitgestelltem Node:
```sh
CTOX_APP_RUNTIME_TEST_NODE=/absolute/path/to/node \
CTOX_APP_RUNTIME_TEST_ROOT=/lane/owned/scratch \
cargo test --manifest-path src/core/app_runtime/Cargo.toml -- --test-threads=2
node --test src/core/app_runtime/handler-kit/handler-kit.test.mjs
```

Die beiden Umgebungsvariablen gelten ausschließlich für Tests. Die Tests starten
echte Node-Prozesse über den wiederverwendeten LocalTransport und prüfen
Lesen/Schreiben, Prozesswiederverwendung, CPU-Schleifenabbruch, Neustart,
Rechteentzug, Foreign-Exports, Migrationen und Ressourcenentzug.
Linux ist das erste geprüfte Ziel; Windows-Named-Pipes und macOS müssen zusätzlich
auf ihren nativen Hosts geprüft werden.

## Übergangsadapter zum bestehenden Core-Store

Mit Cargo-Feature `legacy-store` steht
`legacy_store::LegacyStoreAdapter` zur Verfügung. Der Core übergibt seine
bereits laufende `Arc<RxDatabase>`, seinen Tokio-Handle und eine
`Arc<dyn LegacyStorePolicy>`. Es entsteht keine zweite Datenbank, Replik,
Schreibdienst oder HTTP-Verbindung.

Die Policy hat keine erlaubenden Standardimplementierungen:
`project_read` muss Zeilensichtbarkeit und Feldmasken anwenden;
`prepare_write` muss aktuelle Rollen, geschützte Felder und Schema prüfen
und das erlaubte Dokument zurückgeben (bei erlaubtem Entfernen: None).
Die normalen RuntimeHost-Prüfungen bleiben zusätzlich wirksam.
Diese Implementierung kommt vom Core, niemals aus dem App-Bundle.

Autorisierte Snapshots werden als `ctox.authorized_snapshot.v1` an den
vertrauenswürdigen Node-Einstieg übergeben und dort mit demselben Mango-Code
wie im Handler-Kit ausgewertet. Nicht freigegebene Zeilen/Felder erreichen
den Kindprozess nicht. Es gibt keine implizite Zeilengrenze; ein Snapshot
über 6 MiB schlägt mit quota fehl und wird nicht abgeschnitten. Für größere
Bestände ersetzt der V4-Store diese Übergangsabfrage durch seinen Query-Adapter.

Änderungen laufen über die bestehende native
`RxCollection::incremental_write_queue`. Basisrevision und Dokument-Policy
werden im Modifier bei jedem Konflikt-Retry erneut geprüft. Inserts verwenden
den vorhandenen Insert-Pfad; Entfernen erzeugt einen nativen Tombstone.
Native Fehlerparameter werden nicht zum Kindprozess weitergereicht, da sie
ungemaskte Dokumente enthalten können.

Prüfung des Adapters (ebenfalls nur in der Build-Lane):
```sh
bash src/core/app_runtime/verify.sh legacy-store
```
Der Core muss den Pool dauerhaft halten, registrierte Releases beim
Publish/Uninstall austauschen und seine tatsächliche Actor-/Capability-Policy
anbinden. Die Bibliothek aktiviert sich nicht selbst gegen eine installierte
Instanz oder gegen das alte App-Manifest.
