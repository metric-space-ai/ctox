# Feldbefund Sync: Erst-Pull einzelner Kollektionen endet nie (06.09.2026)

Instanz: thesen.ctox.dev, Release `branch-main-20260905T072559Z`, Shell aus demselben Stand,
Browser: Claude-Browser-Pane (Chromium), frische Anmeldung 06.09. ~18:08 UTC, IndexedDB des
Origins aus Vortagen vorhanden. Gemessen über `window.ctoxBusinessOsSyncDiagnostics()`.

## Beobachtung

Nach dem Öffnen der Outbound-App (18:15:30 UTC) registrieren sich fünf Kollektionen. Drei
werden binnen ~1 min `complete` (sources 27 Docs/23 KB, adapters 14/47 KB,
research_policies). Zwei bleiben > 25 min `pending`, die App zeigt „Kampagnen 0 … Daten werden
synchronisiert (0/5)" und keinen einzigen Lead:

| Kollektion | Docs | KB | readiness | firstPullCompletedAtMs | pullInProgress |
|---|---|---|---|---|---|
| outbound_lead_generation_leads | 54 (35 gelöscht) | 1113, max 118 | `catching-up` | 0 | true |
| outbound_lead_generation_imports | 24 (23 gelöscht) | 12 | `live` (nach manuellem restartCollection) | gesetzt (18:30:47) | true |
| user_thread_states | 3911 | 2342 | `catching-up` seit 18:10 | 0 | true |
| outbound_lead_generation_sources (Vergleich) | 27 | 23 | `live` | gesetzt (18:19:40) | false |

Größe ist es nicht (imports = 12 KB). Alle Dokumente liegen unter dem 256-KB-Draht-Budget.

Weitere Fakten:
- Verbindung stabil: `activePeerCount 1`, `connectedAt 18:16:20`, `roomCircuit closed`, keine
  Fehler, kein `lastRestartReason`, `retryCount 0`, keine Backpressure. Raum-Transport gesamt:
  498 Frames / 5,0 MB empfangen, 1495 Frames gesendet.
- Demand-Loading (Snapshot 18:28, leads): `queryFetchRequests 407`, `queryChunksReceived 451`,
  `queryFetchSuccessCount 0`, `queryFetchErrorCount 0`, `queryFetchInFlight 1`,
  `queryFetchDedupHitCount 33`, `queryDemandLoadingActive true`, `localCoverage full`,
  `syncProfile eager`, `queryReady true`.
- Multi-Tab: dieser Tab ist `leader`, `leaderLeaseAgeMs 0`.
- Browser-Journal: 24 ausstehende Schreibvorgänge / 38 KB, `oldestPendingAtMs` 18:09:35
  (älter als die App-Öffnung), `unresolvedConflicts 0`.
- Natives Journal in den 15 min davor: 5 Zeilen, nur „skipping oversized knowledge item".
- `sync.restartCollection('outbound_lead_generation_leads', …)`: kein Fehler, kein Effekt auf
  `initialReplicationState`; `imports` wurde danach `live`, blieb aber `pending`.
- Vortag (05.09.) identisches Muster für dieselben Kollektionen, damals flossen die Daten
  trotzdem (App zeigte Leads); heute nicht.

## Was das ausschließt

- Draht-Budget/Übergröße (alle Docs < 256 KB, imports 12 KB).
- Verbindungsabbrüche/Neustart-Schleifen (keine Neustarts, stabile Verbindung).
- Größe der Kollektion (imports winzig, user_thread_states groß — beide hängen).

## Hypothesen für die Sync-Engineure (nicht belegt)

1. Der Erst-Pull hängt an einem Checkpoint/Epoch-Stand aus der alten IndexedDB: der Server
   liefert ab einem Stand, den der Browser nie als „aufgeholt" erkennt (`catching-up` ohne Ende).
   Test: gleiche Instanz, frischer Origin-Speicher (IndexedDB löschen) — endet der Erst-Pull?
2. Die 407 Bedarfsabfragen mit 0 Erfolgen: Query-Fetch-Collector wird nie abgeschlossen
   (fehlender Abschluss-Chunk?) und blockiert/verdrängt den regulären Pull.
3. `awaitInitialReplication` verlangt Pull **und** Push; die 24 ausstehenden Journal-Schreibvorgänge
   von 18:09 (vor App-Öffnung) könnten der nie quittierte Push sein.

## Auswirkung

Für den Nutzer: Outbound-App nach Anmeldung minutenlang leer („Kampagnen 0"). Das ist ein
Produktionsproblem der Datenebene, nicht der App.

## Nachtrag 18:37 UTC: Reload heilt die Anzeige, nicht den Zustand

Nach einem harten Neuladen derselben Seite (gleiche IndexedDB) und erneutem Öffnen der App:
„Kampagnen 1 · Chemie 19 · Daten werden synchronisiert (3/5)", alle 19 Leads sichtbar — nach
~70 s. Die Replikationszustände sind dabei unverändert: leads `pending/catching-up`, kein
Erst-Pull; imports `pending/live`; user_thread_states `pending/catching-up`. Die Anzeige kommt
also aus dem lokalen Speicher; der erste Seitenaufruf nach der Anmeldung zeigte 25 Minuten lang
nichts. Für den Nutzer: „nach dem Login leer, nach F5 voll" — reproduzierbar, nicht erklärt.

## Nachtrag 18:53 UTC: Der Push ist in dieser Sitzung tot — Recherchestarts kommen nie an

Fünf Recherchestarts aus der App (18:41 DrinkStar; 18:47 vier weitere) zeigten lokal
„N Recherchen werden gestartet …", erzeugten aber **keinen** Befehl auf dem Server: seit 18:00 UTC
null Zeilen in `business_commands` und in jeder anderen Kollektion, Routing-State leer, kein
Chatfenster, Leads fallen lokal auf „Prüfung nötig" zurück.

Browser-Diagnose `business_commands` (18:53): `initialReplicationState complete`,
`collectionReadinessState never-synced`, `pushInProgress true`, `pullInProgress false`,
`sentFrames 3`, `queuedFrames 1533`, `sentScheduledFrames 1538` (5 Frames nie gesendet = die
fünf Starts), `pendingAcks 0`, `lastAckLagMs 365`, `backpressureStallCount 0`,
`activePeerCount 1`. `commandPlane.counters {}` — kein einziger Befehl gezählt.
Journal: 26 ausstehende Schreibvorgänge / 45,9 KB, ältester 18:09:35 (vor App-Öffnung).
Eine RxDB-`find()` auf `business_commands` hing > 45 s.

Der Pull dagegen funktioniert (19 Leads nach Reload sichtbar). Wirkung für den Nutzer: **Klick
auf „Recherchieren" tut nichts** — der Zustand vom 04.09. („es passiert absolut gar nichts"),
diesmal nicht wegen der App, sondern weil kein Schreibvorgang den Browser verlässt.

Nächster Test: lokalen Speicher (IndexedDB) dieses Browser-Fensters löschen, neu laden, eine
Recherche starten. Landet sie, blockiert der älteste Journaleintrag als Kopf der Schlange.

## Nachtrag 19:01 UTC: Frischer lokaler Speicher ändert nichts → Peer-Sitzung, nicht Browser-Journal

IndexedDB des Origins gelöscht (23 DBs), Seite neu geladen (Sitzung blieb, Shell nach 81 s
bereit, Haupt-DB neu angelegt), App geöffnet:
- nach 267 s: leads `catching-up`, `firstPullCompletedAtMs 0`, 391 Frames / 4,0 MB über den
  Raum empfangen, `retryCount 0`, `resumeRequestCount 0`, 1 Peer; App zeigt 0 Leads.
- Journal: 2 ausstehende Schreibvorgänge (3,7 KB, Login-Schreibvorgänge der Shell) — auf dem
  Server seit 18:50 UTC **null** neue Zeilen in irgendeiner Kollektion.
- Natives Journal: keine Zeile über diesen Browser (kein Handshake, kein Checkpoint).

Damit ist Hypothese 1 (alter Journaleintrag als Kopf der Schlange) widerlegt. Pull kleiner
Kollektionen geht, Pull von leads/user_thread_states endet nie, Push geht gar nicht — mit
frischem und mit altem Speicher. Verdächtig ist die native Peer-Sitzung (läuft seit dem
Upgrade-Neustart 05.09. 07:53). Nächster Schritt: Dienstneustart bei leerer Queue, danach dieselbe
Messung.

## Nachtrag 19:20 UTC: Dienstneustart — Pull heilt, Push-Test läuft

`systemctl --user restart ctox` (SSH-Aufruf hing, der Dienst startete tatsächlich um **19:10:08**;
multiplexed replication für 201 Kollektionen um 19:10:15 oben). Browser: `peer_connection_lost` →
Reconnect 19:05:55 (Signaling) / `business_commands` neu verbunden 19:11:18.

- **Pull nach dem Neustart in Ordnung:** der frische lokale Speicher (0 Leads) füllte sich —
  „Kampagnen 1 · Chemie 19", 19 Leads sichtbar. Der Zustand bleibt trotzdem `catching-up`,
  `firstPullCompletedAtMs 0` (die Zustandsmeldung ist unabhängig davon falsch).
- Ein Recherchestart um 19:10:11 fiel in die Boot-Lücke (vor 19:10:15) und kam nie an — ungültig.
- Neuer Push-Test 19:19:31 (Cereda + DrinkStar, „2 Recherchen werden gestartet …"):
  `business_commands` `pushInProgress true`, gesendete Frames 24 → 27, geplant 3131 / in
  Warteschlange 3128 (3 dauerhaft unversandt), `pendingAcks 0`, Journal 1 ausstehend (Login-Schreib-
  vorgang von 18:54, seit 25 min nicht zugestellt). Serverseitiges Ergebnis: siehe unten.
- Code-Diff der nativen Annahmepfade zwischen Release 04.09. (4f28b66e8) und 05.09. (e9a346e38):
  rxdb_peer.rs 2 Zeilen (Kommentar), store.rs 19 Zeilen (Projektions-Hooks) — kein Push-Pfad
  berührt. Das Release ist nicht die Ursache.
- Nebenbefund im Boot-Log: optionale Kollektion `workjet_computers` wird wegen Schema-Hash-Wechsel
  (RxDB DB6) übersprungen — vorbestehend, unabhängig.

## Nachtrag 19:28 UTC: Sender im Browser steht — Abfragesturm des Threads-Moduls, Push-Frames drainen nie

- Konsole: `[V1.5] fetch:start {collection: ctox_task_approval_requests, fingerprint: bd442eba… | 0836ebb8…, limit 200}`
  im Sekundentakt, 3793 Meldungen verworfen, nie ein Abschluss. Verursacher: das laufende
  Threads-Modul (`modules/threads/index.js:447-448`, zwei `recentQuery`-Ladevorgänge pending/alle
  auf dieselbe Kollektion, die sich offenbar gegenseitig invalidieren).
- `business_commands` frameTransport nach dem Neustart: `sentFrames` **30 und dann konstant**,
  `queuedFrames` 3098 → 3490 in 8 min, `sentScheduledFrames` immer +4 (vier Push-Frames dauerhaft
  unversandt), `pendingAcks 0`, `bufferedAmount` ~260 B, `backpressureStallCount 0`. Empfang läuft
  (Leads kamen nach dem Neustart). Der Sender ruft `send()` schlicht nicht mehr auf.
- `sync.suspendCollections(['ctox_task_approval_requests'])`: ok, aber kein Effekt — `sentFrames`
  bleibt 30, Warteschlange bleibt 3491. Der Sturm ist also Symptom oder Auslöser, nicht der
  einzige Grund: Die Drain-Schleife ist tot (vgl. docs/ctox-rxdb.md „Send-queue wedge",
  `DrainResetGuard` greift hier nicht).
- Zwei weitere Recherchestarts (19:19:31, Cereda + DrinkStar) kamen nie an; Cereda fiel lokal auf
  „Prüfung nötig" zurück.

Offen und entscheidend: Ist das spezifisch für dieses Browser-Fenster (Chromium-Pane) oder trifft
es jeden Browser? Test: Eigentümer startet eine Nachrecherche im eigenen Browser, Server-Poll läuft.

Hinweis für die Sync-Engineure: Auf `origin/main` (06.09.) liegen seit dem Tenant-Release Commits
wie „fix(sync): stop retired background transfers at awaited boundaries", „preserve direct bridges
and isolate DB runtime coordinators" — möglicherweise genau diese Klasse. Nicht verifiziert.

## KORREKTUR 20:20 UTC: Messfehler in allen „seit X"-Abfragen dieses Dokuments

Alle serverseitigen Aussagen der Form „seit 18:00 / seit dem Neustart **null** Zeilen" beruhen auf
`lastWriteTime/1000 >= strftime('%s', …)`. `strftime` liefert TEXT, die linke Seite ist REAL; SQLite
wertet REAL < TEXT, der Vergleich ist **immer falsch**. Eine Abfrage ohne Zeitfilter zeigt vier
`outbound.sellify.lookup`-Befehle als `completed` um 19:44:46, 19:45:26 und 20:06:37 (2×) — aus dem
Browser des Eigentümers, also **kommen Schreibvorgänge aus dem Browser sehr wohl an**.

Damit sind zurückzunehmen bzw. neu zu messen: „Push tot", „kein Befehl seit 18:00", „Recherchestarts
erreichen den Server nie", „frischer Speicher ändert nichts (Server null Zeilen)". **Bestehen bleiben**
die Browser-Zähler (`sentFrames` konstant, Warteschlange wächst, `catching-up` ohne Erst-Pull) und die
Beobachtung „App nach Login 25 min leer, Reload heilt die Anzeige" — sie stammen nicht aus dieser
Abfrage. Die Neu-Messung mit korrektem Vergleich folgt unten.

## Neu-Messung mit korrektem Vergleich (20:33 UTC) — Diagnose: 15–20 Minuten Latenz, nicht „tot"

`typeof(lastWriteTime)=real`, `typeof(strftime('%s',…))=text`; mit `datetime(lastWriteTime/1000,'unixepoch') >= '…'`:

- **24 Befehle seit 19:10:15**, alle `outbound.sellify.lookup` / `completed`, in Schüben:
  18:58:26 (7), 19:06:14 (3), 19:40:50 (4), dazu 19:44:46, 19:45:26, 20:06:37 (2). Das sind die
  Vorabgleiche der Recherchestarts aus meinem Fenster (Klicks 18:41, 18:47, 19:10, 19:19) und aus
  dem Browser des Eigentümers (~20:04). **Round-trip Klick → completed: 17–21 Minuten.**
- **Null `business_os.chat.task`-Aufgaben seit 18:00**, Routing-State und Queue-Projektion heute
  leer. Die App wartet auf den Vorabgleich max. 120 s (`researchLead`, Timeout 120000 für
  Varianten-Läufe) und bricht dann ab: beim Eigentümer als Dialog „Der Sellify-Abgleich ist
  fehlgeschlagen … bitte erneut versuchen" (Screenshot 20:0x), in meinen Sammelläufen still
  (`suppressAlerts`). **Deshalb wird nie eine Recherche eingereiht.**

Zusammengesetzte Ursache (belegt bis auf den letzten Schritt):
1. Das laufende Threads-Modul erzeugt einen Bedarfsabfrage-Sturm auf `ctox_task_approval_requests`
   (zwei `recentQuery`, modules/threads/index.js:447-448; 3793 Konsolenmeldungen, 0 Abschlüsse).
2. Diese Frames füllen die Sendewarteschlange des Browsers (queued 3098 → 3491 in 8 min,
   `sentFrames` nahezu konstant); Befehls-Frames stehen dahinter an → 15–20 min Latenz.
3. Der 120-s-Vorabgleich der App läuft in diese Latenz und bricht ab.
4. Vermutlich derselbe Stau hält den Erst-Pull großer Kollektionen in `catching-up`.

Nicht mehr haltbar: „Push tot" (Korrektur oben). Weiter offen: ob das Schließen des Threads-Moduls
die Warteschlange leert und Recherchestarts wieder binnen Sekunden ankommen (Test läuft).

## 20:42 UTC: Schleife im Threads-Modul belegt — und der Gegenbeweis

`modules/threads/index.js`: `refresh()` (Zeile 442) lädt bei jedem Aufruf `ctox_task_approval_requests`
zweimal (`recentQuery(200, {status:'pending'})` und `recentQuery(200)`), dazu `user_thread_states`
u. a.; `refresh()` wird von einem `setInterval` (Zeile 416), von Bereitschaftswechseln
(`wireReadiness` → `render`) und ~20 weiteren Stellen ausgelöst. Jede `find()` auf einer nicht
`live`-Kollektion ist eine Bedarfsabfrage über den Draht. Solange `user_thread_states` in
`catching-up` hängt, feuern Bereitschaftswechsel dauerhaft → Abfragesturm → Sendewarteschlange voll.

**Gegenbeweis:** Seite neu geladen und — bevor das Threads-Modul startete —
`sync.suspendCollections(['ctox_task_approval_requests','user_thread_states'])` (bei 136 s).
Ergebnis nach 264 s: `outbound_lead_generation_leads` **`live`, `firstPullCompletedAtMs` gesetzt**
(zum ersten Mal heute), App „Daten werden synchronisiert (4/5)", 19 Leads; Sendewarteschlange 490
statt > 3400. Der Stau des Threads-Moduls blockiert also den Erst-Pull anderer Kollektionen und die
Befehlszustellung. Der Recherche-Klick-Test mit leerer Warteschlange folgt.

Eigentümer-Browser (Screenshot ~20:04): „Auswahl neu recherchieren (2)" → „Kein Lead konnte
gestartet werden (2 Fehler): Der Sellify-Abgleich ist fehlgeschlagen … bitte erneut versuchen" =
120-s-Timeout des Vorabgleichs. Um 20:39:15/17 wurden zwei Lookups binnen Sekunden `completed`
(vermutlich erneuter Klick) — im Eigentümer-Browser könnte die Latenz nach dem Dienstneustart
bereits kurz sein; nicht belegt, welcher Dialog dort folgte.

## 20:46 UTC: Beweis — mit ausgesetzten Sturm-Kollektionen kommt ein Recherchestart in 27 s an

Klick „Auswahl nachrecherchieren" (DrinkStar) um **20:45:16** → `outbound.sellify.lookup` `completed`
**20:45:38** → `business_os.chat.task` „Nachrecherche: DrinkStar GmbH" **accepted 20:45:43**, mit
`payload.writeback_contract.mechanism = business_command` (App 1.0.100). Round-trip **27 s** statt
15–20 min. Einziger Unterschied zu den sieben gescheiterten Starts davor: vor dem Start des
Threads-Moduls `sync.suspendCollections(['ctox_task_approval_requests','user_thread_states'])`.

Damit ist die Kette belegt: Threads-Modul-Schleife → Sendewarteschlange voll → Befehlslatenz
15–20 min → 120-s-Vorabgleich der App bricht ab → keine Recherche. Der Sendezähler
`business_commands.frameTransport.sentFrames` (3) ist dabei **kein** verlässliches Maß für
gesendete Befehle (Befehl kam an, Zähler blieb bei 3) — nicht als Beleg verwenden.

Sofortmaßnahme für Nutzer bis zum Fix: Threads-Modul nicht laufen lassen (es startet in der
Leiste als „0 Threads · Läuft" automatisch; ein Schließen-Knopf fehlt). Eigentlicher Fix gehört ins
Threads-Modul (Refresh-Schleife, zwei 200er-Abfragen je Lauf) und/oder in die Bedarfsladung
(Abfragen auf nicht-`live`-Kollektionen dürfen die Befehlszustellung nicht verdrängen).

## Fix (06./07.09.2026)

- **App 1.0.101** (Hotpatch 21:45 UTC): Ein Timeout des Sellify-Vorabgleichs bricht die Recherche nicht
  mehr ab; sie startet ohne Vorwissen (Sellify wird im Lauf erneut geprüft, nichts wird nach Sellify
  geschrieben), der Nutzer bekommt „Sellify hat nicht rechtzeitig geantwortet – Recherche startet ohne
  Vorwissen". Nur ein echter Abgleichsfehler bricht ab; die harte Weiche (followup ohne Treffer) greift
  bei Timeout nicht.
- **Shell `762c4c8da`** (origin/main): `modules/threads/index.js` — `wireRealtime()` pollt nur noch bei
  sichtbarem Modul (`visibilityState`, `host.offsetParent`), nie überlappend (`state.refreshInFlight`,
  Single-Flight in `refresh()`), 30 s statt 10 s, Nachsync bei `visibilitychange`. JS-Suite 117/117,
  Identifier-Guard grün. Auslieferung auf thesen per `ctox upgrade --dev` (startet automatisch, sobald
  keine Recherche geleast ist) — bringt zwangsläufig auch die übrigen Commits von `main` seit dem 05.09.
  mit (`previous_release` bleibt für Rollback).
- Nicht adressiert (Sync-Engine, für die Engineure): Bedarfsabfragen dürfen Befehls-Frames nicht
  verdrängen (Priorität/eigene Bahn); `user_thread_states` wird nie `live`.
