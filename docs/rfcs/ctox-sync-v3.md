# RFC: CTOX Sync v3 — vom Fehlerflicken zur spezifizierten Sync-Engine

Status: Entwurf zur Diskussion · 10.10.2026 · Autor: Claude (Supervisor thesen/CTOX)
Bezug: `docs/ctox-rxdb.md`, `docs/ctox-sync-plan-2026-08-10.md`,
`docs/ctox-sync-production-readiness-95.md`, `docs/ctox-sync-feldbefund-*.md`

## 0. Worum es geht

CTOX Sync verbindet Browser und Rechner mit einer CTOX-Instanz ohne HTTPS-Datenpfad
und ohne Tunnel: WebRTC-DataChannel, RxDB-artige Replikation, SQLite als Wahrheit
auf der Instanz. Diese Grundentscheidung bleibt — sie ist der Grund, warum CTOX auf
beliebigen Rechnern hinter NAT läuft.

Was nicht bleiben kann, ist die Art, wie die Engine weiterentwickelt wird: als
Folge von Einzelfixes für das jeweils letzte Symptom. Der Plan ersetzt das durch
einen festen Vertrag, feste Messbudgets und einen schrittweisen Umbau, bei dem jede
Stufe für sich auslieferbar ist und gemessen wird.

Nicht-Ziele: keine HTTP-Datenbrücke (AGENTS.md, Datengrenze bleibt), kein Ersatz von
WebRTC, kein Big-Bang-Neuschreiben. Umgebaut wird nach dem Strangler-Muster: neuer
Pfad neben dem alten, umschalten je Sammlung, alten Pfad löschen, wenn gemessen.

## 1. Befund mit Zahlen

### 1.1 Änderungsdruck ohne Spezifikation

- 781 Commits seit 01.09.2026 nur in `rxdb/src`, `src/core/rxdb`,
  `rxdb_peer*.rs`, `shared/sync.js`.
- 9 Feldbefund-Dokumente seit 06.09. (Erstpull, Wartungssperre, frischer Tab ohne
  Peer, Start-Stream-Limit, Auth-Grenze …).
- `docs/ctox-rxdb.md` (3.556 Zeilen) beginnt mit ~40 angehängten
  Einzelfall-Abschnitten vor der eigentlichen Architektur. Die Doku ist ein
  Änderungsprotokoll, keine Spezifikation.

### 1.2 Regressionen der letzten Tage (alle gemessen, thesen)

| Datum | Änderung | Wirkung | Warum unbemerkt |
|---|---|---|---|
| 09.10. | Leads auf `syncProfile: demand-only` (dc0a0591b) | Outbound-Start 60 s Timeout, Leads doppelt geblättert, Start-Abgleich übersprungen → 145 s bis „fertig“ | App wartete auf Readiness `live`, die ein demand-only-Profil nie erreicht; kein Ladezeit-Gate |
| 08.–10.10. | Daten wachsen mit der Kampagne | `business_commands` 177 MB, Leads Ø 80 KB, Queue-Tasks Ø 27 KB Prompt | Heiße und kalte Daten liegen in denselben Dokumenten |
| laufend | Byte-Zähler `frameTransport.receivedBytes` | meldet 20 MB, über die Leitung gingen 1,2 MB | Zähler zählt je Sammlungseintrag dieselbe Verbindung |

### 1.3 Was gemessen NICHT das Problem ist

- WebRTC-Rundlaufzeit 22 ms (ICE relay/udp), Server-Antwort auf lokale
  HTTP- und Abfrageanfragen < 3 ms, Last 0,4.
- Kaltstart direkt auf srvki1 (Chromium lokal): Leads 7,1 s, fertig 10,8 s.
- Der DataChannel trägt asynchrone Daten problemlos. Die Probleme entstehen
  oberhalb (Vertrag, Datenmodell, Protokollablauf) und im Netzweg einzelner
  Clients (UDP über TURN 270–980 ms vom Owner-Netz).

## 2. Ursachen

### U1 — Kein expliziter Zustandsvertrag

Eine Sammlung kann eager repliziert, demand-geladen, projiziert oder Steuerkanal
sein. Daraus folgen Zustände (`phase`, `readiness`, `roomCircuit`, Leader/Follower,
Query-Readiness, Demand-Fenster), deren Bedeutung je Profil verschieden ist. Jede
App interpretiert sie selbst (Outbound: `waitForCollectionReadiness` mit 60 s,
`withLeadQueryAuthority`, eigener Lade-Timeout 45 s). Ändert sich das Profil einer
Sammlung, bricht jede App, die die alte Semantik angenommen hat — ohne dass ein
Test es merkt.

### U2 — Drei Speicher, Projektionsketten, doppelte Schreibwege

`ctox.sqlite3` (Kern) → `business-os.sqlite3` (Projektion) → `business-os-rxdb.sqlite3`
(RxDB-Dokumente). Derselbe Zustand existiert in drei Formen; Projektions-Loops
schreiben periodisch (Cockpit-Pass Median 41 ms, p95 2,6 s, max 8,9 s). Folgen:
Sperren (188 „database is locked“ am 08.10., 2 in 10 h nach den Fixes), unklare
Schreibhoheit, schwer reproduzierbare Rennen.

### U3 — Überladenes Datenmodell

Was eine Liste braucht (Name, Status, Zeitstempel) und was nur die Detailansicht
oder der Agent braucht (Belege, Feldstatus, Prompts, Ergebnisse) liegt im selben
Dokument. Abgeschlossene Befehle bleiben voll repliziert. Jede neue Funktion macht
die heißen Pfade schwerer; Projektions-Workarounds (Listen-DTOs, Sidecar-Caches)
behandeln Symptome je App.

### U4 — Ein Kanal, eine Warteschlange, viele Rundreisen

Steuerframes, interaktive Abfragen und Massendaten teilen einen DataChannel. Eine
langsame `business_commands`-Abfrage (3,3 s) hielt die Lead-Seiten auf. Listen
werden seitenweise nacheinander geholt; jede Seite ist eine Rundreise plus
Chunk-Acks. Auf Relay-Netzen multipliziert sich das.

### U5 — Keine Messbudgets, parallele Änderer

Kein Build misst Kaltstart, Neuladen, Abfrage- oder Schreiblatenz gegen echte
Datenmengen. Mehrere Worker (Codex/Workjet) ändern gleichzeitig am Sync; jeder Fix
ist lokal korrekt, die Summe regrediert.

## 3. Zielbild v3

### 3.1 Sync-Vertrag (Spezifikation statt Implementierungsdetail)

Jede Sammlung hat genau ein Profil, deklariert im Schema-Vertrag
(`src/core/rxdb/tests/fixtures/*.json`, beidseitig generiert):

| Profil | Datenfluss | „bereit“ heißt | Schreiben |
|---|---|---|---|
| `replicated` | vollständig, Push + Pull | erster vollständiger Pull bestätigt | lokal sofort, bestätigt mit Master-Ack |
| `demand` | nur abgefragte Fenster | erste autoritative Antwort auf die konkrete Abfrage | nur über Befehl oder bestätigten Write |
| `control` | Befehle/Status, append-only | Steuerkanal offen | ausschließlich über Command-Bus |
| `stream` (neu) | Ereignisse ohne Historie (Präsenz, Fortschritt) | Kanal offen | nicht persistiert im Browser |

Apps sehen genau eine Schnittstelle: `ctx.data.ready(collection, query?)`,
`ctx.data.read(...)`, `ctx.data.write(...)`, `ctx.data.subscribe(...)` mit der
obigen Semantik. Kein App-Code wertet `phase`, `readiness`, `roomCircuit` oder
Query-Readiness direkt aus. Ein statischer Wächter verbietet das (wie
`assert-rxdb-only`).

### 3.2 Datenklassen

- **Listenfelder** (heiß, klein): repliziert oder als serverseitige Projektion.
- **Detail/Belege** (kalt, groß): `demand`, je Datensatz nachgeladen.
- **Prompts, Ergebnisse, Verläufe abgeschlossener Arbeit**: nicht im Browser,
  nur auf Abruf.
- **Aufbewahrung**: abgeschlossene `business_commands` nach N Tagen aus der
  Replikation (Archiv bleibt im Kern abrufbar).

Budget: kein repliziertes Dokument > 8 KB, keine eager-Sammlung > 2 MB im
Neuladen-Pfad einer App.

### 3.3 Eine Wahrheit, ein Schreiber je Datum

Jedes Datum hat genau einen Eigentümer-Speicher. Projektionen werden zu einem
einzigen Projektions-Schreiber mit Batching und fester Taktung zusammengefasst;
keine zweite Kopie in einer Zwischendatenbank für Daten, die nur der Browser
liest. Ziel: Projektionspass p95 < 200 ms, null „database is locked“.

### 3.4 Transport

- **Getrennte Spuren**: je ein DataChannel (eigener SCTP-Stream, kein
  Head-of-line) für `control`, `interactive` (Abfragen der sichtbaren App) und
  `bulk` (Pull, Dateien). Die sichtbare App hat Vorrang vor Hintergrund-Apps.
- **Gebündelte Abfragen**: eine Liste in einer Anfrage mit Server-Streaming
  statt seitenweise Rundreisen.
- **Netzwege**: TURN über TCP/TLS 443 als automatischer Rückfall, wenn der
  UDP-Weg schlecht ist (bleibt WebRTC, keine HTTP-Daten).
- **Warmstart**: Sitzungs- und ICE-Wiederaufnahme, damit ein Neuladen nicht den
  vollen Aufbau zahlt.

### 3.5 Beobachtbarkeit

Ein Startprotokoll je Ladevorgang (Phasen der Shell, erste Antwort je Sammlung,
App bereit) in `CTOX_BUSINESS_OS_STATUS`; echte Byte-Zähler je Verbindung und
Spur; serverseitig Latenz-Histogramme je RPC. Das ist die Datengrundlage der
Gates in §4.

## 4. Messbudgets (Gates)

Benchmark-Datensatz mit thesen-Größe (850 Leads à 80 KB, 6.000 Befehle, 400
Queue-Tasks, 2.000 Chats) als Fixture; gemessen lokal auf dem Server und über
einen gedrosselten Relay-Pfad (300 ms RTT).

| Messgröße | Budget |
|---|---|
| Neuladen, App interaktiv (Daten sichtbar) | ≤ 3 s lokal, ≤ 6 s Relay |
| Neuladen, App „fertig“ | ≤ 6 s lokal, ≤ 12 s Relay |
| Kaltstart frischer Browser, App fertig | ≤ 12 s lokal, ≤ 30 s Relay |
| Abfrage p95 (interaktive Spur) | ≤ 300 ms lokal |
| Schreib-Bestätigung p95 | ≤ 1 s |
| Bytes bis „App interaktiv“ | ≤ 2 MB |
| „database is locked“ | 0 je 24 h |

Jedes Release und jede Sync-Änderung zeigt die Messung vorher/nachher. Wer ein
Budget reißt, merged nicht. Vor jedem Tenant-Deploy läuft dieselbe Messung live.

## 5. Vorgehen in Stufen

Jede Stufe ist eigenständig auslieferbar, endet mit Messung und lässt das System
in einem besseren, nie in einem halben Zustand.

| Stufe | Inhalt | Ergebnis / Gate |
|---|---|---|
| **S0 Messfundament** (zuerst) | Benchmark-Fixture, Mess-Harness (`measure-*`-Skripte als Repo-Werkzeug), Gate im Release-Skript, echte Zähler, Startprotokoll | Ist-Werte aller Budgets dokumentiert; Release blockiert bei Regression |
| **S1 Vertrag** | Profil-Tabelle §3.1 im Schema-Vertrag, `ctx.data.ready/read/write/subscribe` in Shell + Runtime, Wächter gegen direkte Zustandsauswertung | Outbound, Crew, Mail, Sellify auf neue API migriert; App-eigene Readiness-Logik gelöscht |
| **S2 Datenklassen** | Listenprojektion serverseitig, Belege/Prompts `demand`, Aufbewahrung für Befehle | Bytes bis „interaktiv“ ≤ 2 MB; Dokumentgrößen-Budget erzwungen |
| **S3 Transport** | Spuren, gebündelte Abfragen, TURN-TCP-Rückfall, Warmstart | Relay-Budgets erfüllt |
| **S4 Ein Schreiber** | Projektionskette zusammenführen, Schreibhoheit je Datum | 0 Sperren, Projektionspass p95 < 200 ms |
| **S5 Doku als Spezifikation** | `ctox-rxdb.md` neu geschnitten: Vertrag, Architektur, Protokoll; Einzelfälle in ein Änderungsprotokoll | Doku beschreibt den Ist-Vertrag vollständig |

Reihenfolge-Begründung: S0 macht jede weitere Stufe messbar. S1 beseitigt die
Klasse von Regressionen, die heute am teuersten ist (App bricht bei
Profiländerung). S2 vor S3, weil weniger Bytes jede Transportfrage entschärfen.
S4 zuletzt, weil es den Kern berührt und den größten Testaufwand hat.

## 6. Arbeitsweise (gegen die Regressionsfalle)

1. **Ein Owner** für Vertrag und Protokoll. Änderungen daran nur per RFC mit
   Messbeleg.
2. **Keine App-Workarounds** für Sync-Symptome ohne Ticket an den Sync-Owner;
   App-seitige Zwischenlösungen (z. B. Outbound-Zwischenspeicher) werden in S1/S2
   zurückgebaut.
3. **Ein Sync-Worker zur Zeit** im Kern (`rxdb/src`, `src/core/rxdb`,
   `rxdb_peer*`, `sync.js`); parallele Worker nur in getrennten Stufen.
4. **Feldbefunde** gehen als Messung + reproduzierender Test in den
   Benchmark, nicht als neuer Abschnitt oben in die Doku.
5. **Jede Stufe endet mit Live-Messung** auf thesen und welsch.

## 7. Offene Entscheidungen

- Aufbewahrungsfrist für abgeschlossene Befehle im Browser (Vorschlag 7 Tage).
- TURN über TCP/TLS 443 für alle Tenants freigeben (Kosten Cloudflare-TURN).
- Wer ist Sync-Owner (eine Sitzung/ein Worker-Strang, nicht wechselnd)?
- Migration bestehender IndexedDB-Bestände: verwerfen und neu laden (einfach)
  oder migrieren (aufwendig) — Vorschlag: verwerfen, da alles autoritativ auf der
  Instanz liegt.
- Zeitrahmen: S0 ~1 Woche, S1 ~2 Wochen, S2 ~2 Wochen, S3 ~2 Wochen, S4 ~3
  Wochen, S5 begleitend.
