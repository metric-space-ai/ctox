# Native App-Laufzeit (Backend)

Ein dauerhaft beaufsichtigter Node-Prozess pro App, ein Core-Store und dessen
vorhandener Writer, ausschließlich LocalTransport. Die ESM-Handler und das
separat nutzbare Handler-Kit benötigen keinen JS-Build oder Paketdownload.

## Produktive Einbindung

`AppRuntimeService::production(root, Arc<CoreRuntimeHost>)` ist der
Core-Einstieg. Der Core übergibt seinen vorhandenen Store-Writer, ReadPool,
Tokio-Multi-Thread-Handle, die tatsächliche Autorität und die CAP-1-Registry.
Es wird keine zweite Datenbank und kein weiterer Schreibdienst angelegt.

Die API ist synchron; der Core ruft sie auf seinem begrenzten Blocking-Executor
auf, niemals mit gehaltenem Writer-/Change-Stream-Lock. Er verbindet folgende
Hooks nach seinem Commit, außerhalb des Writers:
- `published(app_id, app_ver)` beim Start aus den aktiven App-Versionen und
  nach Publish/Aktivierung. Die Version kommt über `StoreCodec::version`
  aus business_app_versions; Quellbytes kommen ausschließlich aus dem
  Transfer-CAS über `artifact(hash)`. Jeder Dateihash wird geprüft.
- `command(command, fence)` mit authentifiziertem vollständigem Befehl und
  tatsächlich gehaltenem Executor-Fence.
- `change(ChangeEvent {seq,collection,op,record_id})` aus dem geordneten
  Store-Änderungsstrom; keine Tabellenabfrage als Trigger-Ersatz.
- `tick()` aus dem Core-Timer; Manifestintervalle mindestens PT1M.
- `completed(binding,task_id,succeeded,outcome)` aus dem dauerhaften
  Task-Abschluss. Die Referenz stammt aus der gespeicherten Task-Bindung,
  nicht aus einem frei gewählten Callback-Pfad.
- `migrate(command,fence,collection,docs)` erzeugt autorisierte Vorschläge
  aus der zugelassenen CAS-Version; Übernahme bleibt im Core-Writer mit
  erneuter Basisrevisions-/Generationsprüfung.
- `removed(app_id)` bei Entfernung/Widerruf.

Neue Versionen warten auf den laufenden Aufruf und ersetzen danach die
Prozessgeneration. Abschlüsse bleiben an die ursprüngliche unveränderliche
Version gebunden; die Laufzeit wechselt dafür unter App-Sperre vorübergehend
zur alten Version und anschließend zurück. Keine parallelen Generationen,
kein automatisches Replay fehlgeschlagener Effekte.

## Store- und Autoritätsvertrag

`CoreRuntimeHost` konsumiert die Store-Crate. Der Pflichtadapter
`StoreCodec` besitzt die tatsächlichen Tabellen, Revisionen, Generationen,
Zeilen-/Feldmasken, Schema-Prüfungen und atomaren Revision-/Lease-Vergleiche.
Die Laufzeit erfindet kein zweites App-Datenschema. Folgende Pflichten bleiben
beim Core-Integrator:
- `CoreAuthority::admit`: command_rights und App-Schreibrechte **vor** Code,
  authentifizierter Actor statt behaupteter Flags, aktuelle Grants/Fences.
- `authorize/exported`: aktuelle Rechte und beidseitige Fremd-Freigabe pro
  Operation. Watch-Export wird vor jedem Fremd-Trigger neu geprüft.
- `StoreCodec::check_fence`: tatsächlicher Lease-Datensatz in derselben
  Writer-Transaktion wie jeder Effekt; ein vorgelagerter Check reicht nicht.
- `StoreCodec::write`: ganze Batch atomar, Basisrevision/Generation vergleichen,
  native Revision erzeugen, geschützte Felder bewahren.
  Remove schreibt einen nutzlastfreien Tombstone und löscht in derselben
  Transaktion Revisions-/Archivkörper gemäß Architektur §16.23.
- `StoreCodec::platform`: dauerhafte Annahme von Folgebefehlen mit
  id/record_id/module/client_context und tatsächlichem task_id.
- `version/artifact`: ausschließlich zugelassene unveränderliche Release-Daten.

Die Daten-Transaktion hält während des JS-Callbacks keine SQLite-Sperre.
Ein autorisierter Snapshot und ein Core-Stempel werden an den Aufruf gebunden.
Alle staged Writes laufen in **einem** Writer-Job mit erneuter CAS-, Policy-,
Export- und Fence-Prüfung. Schreibzugriffe auf nur schreibbare Fremd-Collections
werden ohne Datensnapshot geplant; tx.read bleibt dafür verboten.
Externe Effekte sind darin gesperrt.
Der aktuell bereitgestellte Snapshot-Query-Adapter wertet Mango einschließlich
skip ohne impliziten Zeilendeckel aus; oberhalb des IPC-Budgets folgt explizit
quota, niemals stille Kürzung. Ein nativer Query-Adapter für große Bestände
bleibt eine Store-Integrationsaufgabe.

Nur generische Ausführungsmetadaten gehören dieser Bibliothek:
`_app_runtime_triggers` und `_app_runtime_completions`, angelegt und geändert
über denselben Store-Writer. Sie speichern Cursor, seq, Fälligkeit, Fence,
Laufzustand und Callback-Referenzen, keine App-Dokument- oder Ergebnis-Kopien.
Trigger-Claim verhindert parallele/replayte Ausführung; Cursor benötigt einen
Core-Trigger-Kontext. Task-Abschluss wird höchstens einmal beansprucht.
Nach Absturz bleibt ein laufender Claim zur Core-Reconciliation erhalten;
nicht idempotente Effekte werden nicht blind wiederholt. Eine Annahme ohne
nachträglich gespeicherte Callback-Bindung liefert uncertain, kein Replay.

`RegistryRouter` konsumiert das CAP-1-Protokoll cap.invoke/cap.result und die
produktive Registry samt Policy, Fence und Audit. Der native Descriptor bestimmt
Namespace, Funktion und geschlossenes Argumentformat. Nicht installierte
Funktionen liefern unavailable. agent.task.enqueue ist der registrierte
Action-Adapter und benötigt dessen action; keine eigene Capability-Abkürzung.
ctx.model und ctx.http benötigen die nativen Registry-Deskriptoren model.invoke
bzw. http.invoke (Request als payload). Fehlen diese, folgt unavailable; der
Runtime-Adapter ersetzt sie nicht durch eine fremde Capability oder Netzroute.

Diese Bibliothek aktiviert sich nicht selbst im Daemon. Core-Bootstrap,
konkrete StoreCodec/CoreAuthority-Implementierung und Feed-/Timer-Anschluss
liegen außerhalb der exklusiven app_runtime-Dateien und müssen bei der
Core-Integration angeschlossen werden. Die isolierte Prüfung ist keine
Abnahme des installierten Stacks.

## Node-Paket und Prozessgrenze

Produktionsbuild mit Feature `bundled-node` und dem **Build-Eingang**
`CTOX_APP_RUNTIME_NODE_BUNDLE=/absolute/path/to/verified/native/node`.
Die Datei wird mit include_bytes! eingebettet. Das Paket muss passend für den
Zielhost gebaut und über den normalen signierten Release-Weg geliefert werden.
Zur Laufzeit wird die hashadressierte Binärdatei mit Eigentümerrechten
materialisiert und erneut gegen die eingebetteten Bytes geprüft.
Kein Node aus PATH, kein Produktionsdownload, keine Runtime-Umgebungsschalter.
Ohne eingebettetes Node schlägt der produktive Konstruktor geschlossen fehl.

Node >=25 mit Netz-Permissions startet mit leerer Umgebung, --permission ohne
allow-Liste und --max-old-space-size=256. Der Core vererbt nur private
LocalTransport-Handles als stdin/stdout; zusätzliche Unix-FDs werden geschlossen.
Kein HTTP-/TCP-Ersatzpfad. Handler erhalten keine Datei-/Netz-/Prozessrechte.
Die Permissions allein sind keine Sicherheitsgrenze gegen bösartigen Code:
nur vom Core verifizierte Releases dürfen geladen werden.

Handler: handle(command,ctx), relative statische Importe innerhalb handlers/,
Bibliotheken mit _-Präfix, benannte Abschluss-Exporte. Gemeinsame ctx-Fabrik
und Mango-Auswertung mit dem Handler-Kit. Globals und vollständige API siehe
handler-kit/README.md. Migrationen liefern nur Vorschläge; der Core-Writer
prüft Basisrevision/Zielgeneration nochmals vor Commit.

Grenzen: 256 MiB V8-Heap (keine RSS-Garantie), 4 MiB Quellen,
1 MiB Eingabe/Ergebnis, 8 MiB IPC-Rahmen, 6 MiB autorisierte Snapshots.
Der Watchdog beendet ausschließlich das eigene Node-Kind bei 30 Sekunden
tatsächlicher CPU pro Aufruf; Core-I/O-Wartezeit zählt nicht dazu.
Zusätzlich zehn Minuten Wall-Zeit. Nach Crash wird beim nächsten zugelassenen
Aufruf neu gestartet, ohne ungeprüften Hintergrund-Spawn. Zehn Minuten Idle
führen zum Reaping. Entfernen/Widerruf unterbricht; Publish drainiert.
Core-Callbacks bleiben kurz und dürfen nicht synchron denselben Pool aufrufen.

## Prüfung und Mutationsnachweis

Ausschließlich in der zugewiesenen Linux-Lane:
```sh
ctox-prep.sh &&
cargo check --manifest-path src/core/app_runtime/Cargo.toml &&
bash src/core/app_runtime/verify.sh bundle &&
python3 src/core/app_runtime/mutation-proof.py \
  --out "$CARGO_TARGET_DIR/app-runtime-mutation-proof"
```
Der Prüfer provisioniert ausschließlich für Entwicklung das hashgeprüfte
Node-Archiv. Der Mutationslauf verändert gezielt Vertragssicherungen, verlangt
einen passenden roten Assertion-Test und stellt alle Dateien wieder her.
proof.json enthält Quellzeilen, Mutationen, zugeordnete Tests und Logs;
abschließend werden die wiederhergestellten Quellen erneut geprüft.
Eine Compile-/Syntaxfehlermeldung zählt nicht als Mutationsnachweis.

Die Prüfung nutzt echte Node-Prozesse, LocalTransport, SQLite-Writer/ReadPool
und die CAP-1-Registry. Der Test-Codec besitzt isolierte Testtabellen, kein
Tenant und kein Modell. Native macOS-/Windows-Prüfung, Core-Einbindung und
Regressionen am realen installierten System bleiben gesondert nachzuweisen.

## Vorhandener Übergangsadapter

Das optionale Feature legacy-store erhält den bisherigen RxDatabase-Adapter
für die Vorgängerversion. Es ist kein Ausweichpfad der V4-Produktion.
Dessen LegacyStorePolicy muss Zeilen/Felder und Writes im nativen Modifier
prüfen. Separater bestehender Prüfweg: verify.sh legacy-store.
