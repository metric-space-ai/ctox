# RFC: CTOX Sync v3 — vom Fehlerflicken zur spezifizierten Sync-Engine

Status: Entwurf v2 zur Umsetzung · 10.10.2026
Owner Vertrag/Gates: Claude-Sitzung „Outbound app Funktionsproblem (fork)“ ·
Supervisor: Claude-Sitzung „Supervising ctox, workjet, ctox-dev“ ·
Umsetzung (einziger Strang): Codex-Parent „Architektur Rework“ (01a0879f-d2e0-72c3-9353-4ef802b2998b)
Belege: `docs/rfcs/ctox-sync-v3-evidence/root-causes-2026-10-10.md` und
`classification-2026-10-10.csv` (366 klassifizierte Fixes), Messungen thesen 10.10.2026.

## 0. Ziel und Grenzen

CTOX Sync verbindet Browser und Rechner mit einer CTOX-Instanz ohne HTTPS-Datenpfad und
ohne Tunnel (WebRTC-DataChannel, RxDB-artige Replikation, SQLite auf der Instanz). Diese
Grundentscheidung bleibt. Was endet, ist die Weiterentwicklung als Folge von Einzelfixes.

Die Ursachen sind nicht vermutet, sondern aus der vollständigen Fix-Historie abgeleitet
(§1). Jede Architekturänderung in §3 ist einer Ursachenklasse mit gezählten Fixes
zugeordnet; die Reihenfolge in §5 folgt dem Anteil künftiger Fixes, den sie verhindert,
und ihrem Risiko.

Nicht-Ziele: keine HTTP-Datenbrücke (AGENTS.md), kein Ersatz von WebRTC, kein
Big-Bang-Neuschreiben. Umbau nach Strangler-Muster: neuer Pfad neben dem alten,
Umschalten je Sammlung, alten Pfad löschen, wenn gemessen.

## 1. Befund

### 1.1 Fix-Historie (01.09.–10.10.2026)

781 Commits an Sync-Pfaden, davon **366 echte Fixes** (Rest: 206 Features, 118 nur Tests,
52 Domänen-/UI-Fixes, 39 Doku/Format/Stempel). **Die Fix-Rate steigt**: September 6,6 je
Tag, 01.–10.10. 16,9 je Tag (05.–10.10. allein 142). 72 % der Fixes von Codex-Workern,
7 als `[UNVERIFIED]` gelandet. Der Fix-Hotspot `store.rs` (48.808 Zeilen, 101 Fixes) lag
bisher nicht einmal im Sync-Pfadbegriff.

| Klasse | Fixes | % | Kern der Ursache |
|---|---:|---:|---|
| U2 Speicher/Projektionen ohne Schreibhoheit | 59 | 16,1 | mehrere Schreiber je Datum, Projektionen schreiben RxDB-Tabellen direkt, Schatten-/Kompat-Tabellen als Rückfall-Lesequellen |
| AUTH Rechte an jeder Await-Stelle | 43 | 11,7 | Autorität wird je Handler/Fenster neu geprüft statt einmal je Verbindung festgelegt |
| ASSET Cache-Stempel von Hand | 36 (+17 reine Bumps) | 9,8 | `?v=`/Shell-Generation global und manuell gepflegt |
| U1 impliziter Profil-/Readiness-Vertrag | 31 | 8,5 | das Profil einer Sammlung steuert still sechs Verhalten |
| GEN Verträge/Register doppelt von Hand | 29 | 7,9 | Befehlsinventar, Allowlisten, Schemata JS↔Rust driften |
| LIFE Ressourcen-Lebenszyklus | 28 | 7,7 | Slots, Leases, Beobachter, Timer ohne Besitzer |
| U4 Transport | 24 | 6,6 | Sitzungs-/Responder-Lebenszyklus unspezifiziert, Flusskontrolle beidseitig verschieden; Head-of-line nur ~6 Fälle |
| U5 parallele Änderer | 22 | 6,0 | Integrationsreparaturen nach parallelen Merges |
| CMD Command-Bus-Semantik | 18 | 4,9 | Idempotenz, Endzustände, Doppelzustellung |
| OBS nachgerüstete Diagnose | 17 | 4,6 | Messwerkzeuge erst nach Vorfällen, teils falsch |
| U3 Dokumentgröße/Wachstum | 13 | 3,6 | wirkt vor allem indirekt (Folgefehler der Größenbegrenzung) |
| QRY Abfrage-/Invalidierungsstürme | 12 | 3,3 | gehört zu U1 |
| SQL Verbindungen/Transaktionen | 9 | 2,5 | gehört zu U2 |
| REPL Checkpoints, Uhren, falsche Acks | 8 | 2,2 | |
| SCHEMA Versions-/Migrationsdrift | 6 | 1,6 | |
| IDB IndexedDB-Journal/Cache | 6 | 1,6 | |
| MTAB Multi-Tab | 4 | 1,1 | gehört zu U1 |

Unsicherheit ±15–20 % je Klasse (ein Klassifizierer, 75 % der Fix-Commits ohne Text).

Zusätzlich vom Supervisor aus dem Betrieb belegt und hier zugeordnet: Abfragesemantik-Fallen
(`find()` kappt bei 200, REAL- vs. TEXT-Zeitvergleich, nicht ausgewertete Löschmarken) → U1/QRY;
Demand-Sidecar-LRU verdrängt eager Zeilen → IDB/U1; Live-SQLite per Datei kopiert/geöffnet,
Watchdog-Neustart mitten in Wartung → LIFE/SQL; Slot-/Binary-/Asset-Drift beim Deploy →
ASSET; falsche Zähler und Netzweg-Messfehler → OBS.

### 1.2 Die tiefsten Fix-Ketten (Beispiele)

- Projektionsschreiber halten den SQLite-Schreib-Lock: 13 Fixes in 33 Tagen, noch am 10.10.
- RxDB-Hüllen nach ungültigen Projektionsschreibvorgängen repariert: 8 Fixes an einem Tag.
- Größenlimit in der Projektion (`e87de4f50`) → Datei-Platzhalter (`88b4af3f8`) → 12
  abgebrochene Peer-Starts (`17b12b15f`).
- eager↔demand-Profil: ≥10 Fixes in Folge (`ee2259f7d` → … → `dc0a0591b` → `1773924d1`),
  zuletzt 145 s Outbound-Start (thesen 10.10.).
- #211 Steuerebenen-Rechte: 10 Fixes in 5 Tagen, je Review eine weitere Await-Stelle.
- Shell-Stempel: 27 Fixes an 16 Tagen, zwei Tenant-Boot-Ausfälle.

### 1.3 Was gemessen nicht das Problem ist

WebRTC-RTT 22 ms, Server-Antwort < 3 ms, Kaltstart lokal auf srvki1 7–11 s, 1,2 MB bis zur
ersten Lead-Zeile. Der DataChannel trägt asynchrone Daten. Langsam ist der UDP-Weg einzelner
Clients über TURN (Owner-Netz 270–980 ms).

## 2. Ursachen (nach Hebel geordnet)

- **K1 Kein Eigentümer je Datum (U2+SQL, ≈20 %)**: Derselbe Zustand lebt in `ctox.sqlite3`,
  `business-os.sqlite3` und `business-os-rxdb.sqlite3`; mehrere Schreiber, Projektionen am
  RxDB-Dokument-API vorbei, alte Tabellen als Lese-Rückfall. Jeder Fix behandelt einen
  Schreibweg lokal.
- **K2 Profil ist kein Vertrag (U1+QRY+MTAB, ≈13 % + 35 App-Kompensationen)**: Das Profil
  einer Sammlung entscheidet still über Lesepfad, Readiness, Push, Verdrängung,
  Checkpoint-Gültigkeit und Follower-Tab-Bedienung. Apps raten die Semantik.
- **K3 Autorität ohne Kontext (AUTH, ≈12–16 %)**: Rechte werden an jeder Await-Stelle neu
  geprüft; prozessweite Sperren als Notbehelf.
- **K4 Handgepflegte globale Singletons (ASSET+GEN, ≈18 %)**: Cache-Stempel, Befehlsinventar,
  Allowlisten, Schemata doppelt von Hand → Defekte und die größten Merge-Konflikte (→ U5).
- **K5 Lebenszyklus und Transport ohne Zustandsmaschine (LIFE+U4+REPL, ≈16 %)**.
- **K6 Datenmenge (U3)**: kleine direkte Klasse, aber Auslöser von Folgefehlern und der
  Ladezeit (`business_commands` 177 MB, Leads Ø 80 KB, Prompts Ø 27 KB).
- **K7 Prozess (U5+OBS)**: parallele Änderer, keine Gates, unvalidierte Messwerkzeuge.

## 3. Zielbild

### 3.1 Profilvertrag (gegen K2)

Ein Profil je Sammlung im generierten Schemavertrag, jedes mit allen sechs Verhalten
ausdrücklich festgelegt:

| Profil | Lesepfad | „bereit“ | Push | Verdrängung | Checkpoint | Follower-Tab |
|---|---|---|---|---|---|---|
| `replicated` | lokal nach Vollpull | Vollpull bestätigt | lokal + Master-Ack | nie | gültig | vom Leader gespiegelt |
| `demand` | autoritative Abfrage, Fenster-Cache | Antwort auf die konkrete Abfrage | nur Befehl/bestätigter Write | LRU nur eigene Fenster | je Fenster | eigene Abfrage über Leader |
| `control` | Befehlsstatus | Kanal offen | nur Command-Bus | nie | Append-only | gespiegelt |
| `stream` | flüchtige Ereignisse | Kanal offen | nein | sofort | keiner | eigener Abonnent |

Datenzustand ist dreiwertig: `known` / `unknown` / `stale` — „leer“ und „noch nicht
geladen“ sind nie dasselbe. Apps nutzen nur `ctx.data.ready/read/write/subscribe`; ein
statischer Wächter verbietet das Auswerten von `phase`, `readiness`, `roomCircuit` oder
Query-Readiness in App-Code. Abfragesemantik (Limits, Zeitvergleiche, Löschmarken) ist Teil
des Vertrags und getestet.

### 3.2 Ein Eigentümer je Datum, ein Projektionsschreiber (gegen K1)

Eigentum je Sammlung und Feld im Schemavertrag; der native Peer lehnt Schreibvorgänge von
Nicht-Eigentümern endgültig ab; der Browser schreibt nie servereigene Felder. Genau ein
gebündelter Projektionsschreiber schreibt RxDB-Tabellen, und zwar über das
RxDB-Dokument-API. Schatten- und Kompat-Tabellen werden als Lesequellen stillgelegt. Eine
langlebige Schreibverbindung mit IMMEDIATE-Transaktionen. Umgesetzt Sammlung für Sammlung,
`business_commands` zuerst.

### 3.3 Sitzungskontext mit Fähigkeiten (gegen K3)

Autorität wird je Verbindungsgeneration einmal in einen unveränderlichen Kontext aufgelöst.
Handler und Fenster deklarieren ihren Bedarf; das Framework prüft bei Annahme und bei
Veröffentlichung; Cache-Schlüssel enthalten den Rechte-Digest. Keine prozessweiten Sperren.

### 3.4 Generiert statt gepflegt (gegen K4)

Ein beim Build erzeugter, inhaltsgehashter Modulgraph (Import-Map) ersetzt jedes `?v=` und
jeden Generationsstempel. Befehlsinventar, Allowlisten und Schemadarstellungen werden aus
einem Register erzeugt; CI prüft nur noch „generiert = eingecheckt“.

### 3.5 Lebenszyklus und Transport (gegen K5)

Strukturierte Nebenläufigkeit: jeder Slot, jede Lease, jeder Beobachter und Timer gehört
einer Verbindungsgeneration oder einem View-Scope und endet mit ihm. Eine spezifizierte
Zustandsmaschine für WebRTC-Sitzung und Responder, ein gemeinsames Annahme- und
Flusskontrollprotokoll für beide Seiten, getrennte Spuren (`control`, `interactive`, `bulk`)
als eigene DataChannels, gebündelte Abfragen statt Seiten-Rundreisen, TURN über TCP/TLS 443
als Rückfall, Sitzungs-/ICE-Wiederaufnahme beim Neuladen.

### 3.6 Datenklassen (gegen K6)

Listenfelder repliziert oder serverseitig projiziert; Belege, Feldstatus, Prompts und
Ergebnisse `demand`; abgeschlossene Befehle nach Frist aus der Replikation (Archiv im Kern).
Budget: kein repliziertes Dokument > 8 KB.

### 3.7 Beobachtbarkeit mit geprüften Instrumenten (gegen K7)

Startprotokoll je Ladevorgang, Byte-Zähler je Verbindung und Spur, RPC-Latenzen
serverseitig. Jedes Instrument hat einen Selbsttest gegen eine unabhängige Messung (Beispiel:
`frameTransport.receivedBytes` meldete 20 MB bei 1,2 MB auf der Leitung).

## 4. Messbudgets (Gates)

Benchmark-Fixture in thesen-Größe (850 Leads à 80 KB, 6.000 Befehle, 400 Queue-Tasks, 2.000
Chats); gemessen lokal und über gedrosselte Relay-Pfade mit 300 ms und 600 ms RTT.

| Messgröße | Budget |
|---|---|
| Neuladen, Daten sichtbar | ≤ 3 s lokal, ≤ 6 s Relay 300 ms |
| Neuladen, App fertig | ≤ 6 s lokal, ≤ 12 s Relay 300 ms |
| Kaltstart frischer Browser, App fertig | ≤ 12 s lokal, ≤ 30 s Relay 300 ms |
| Abfrage p95 (interaktive Spur) | ≤ 300 ms lokal |
| Schreib-Bestätigung p95 | ≤ 1 s |
| Bytes bis „Daten sichtbar“ | ≤ 2 MB |
| Browser-Heap / IndexedDB nach Neuladen | festgelegt in S0 nach Ist-Messung |
| Server-CPU je Projektionspass, Schreibverstärkung | festgelegt in S0 |
| „database is locked“ | 0 je 24 h |

Jede Sync-Änderung belegt die Messung vorher/nachher im PR. Wer ein Budget reißt, merged
nicht. Vor jedem Tenant-Deploy dieselbe Messung live (welsch nicht vor Mo 12.10. 13:00).

## 5. Stufen

| Stufe | Inhalt | Gate / Ergebnis | verhindert |
|---|---|---|---|
| **S0 Messfundament** | Benchmark-Fixture, Mess-Harness im Repo, Gate im Release, geprüfte Instrumente, Startprotokoll | Ist-Werte aller Budgets; Release blockiert bei Regression | K7 |
| **S1 Generiert statt gepflegt** | Modulgraph mit Inhaltshash, generierte Inventare/Allowlisten/Schemata | keine `?v=`-Commits mehr; Konfliktquelle weg | K4 (≈18 %) |
| **S2 Profilvertrag + Eigentum deklariert** | Profile §3.1 mit sechs Verhalten und Tri-State im Vertrag; Eigentum je Feld deklariert; `ctx.data`-API; Wächter; Apps migriert (Outbound, Crew, Mail, Sellify); **ein Schreiber für `business_commands`** vorgezogen | App-eigene Readiness-Logik gelöscht; 0 Sperren durch `business_commands` | K2 (≈13 %), Teil K1 |
| **S3 Sitzungskontext** | Autorität je Verbindungsgeneration, deklarierter Bedarf, Rechte-Digest in Cache-Schlüsseln | prozessweite Sperren entfernt | K3 (≈12 %) |
| **S4 Ein Projektionsschreiber** | Sammlung für Sammlung auf Eigentümer + Einzel-Schreiber, Schattentabellen stilllegen, Datenklassen §3.6 | 0 „database is locked“, Projektionspass im Budget | K1 (≈20 %), K6 |
| **S5 Lebenszyklus und Transport** | Strukturierte Nebenläufigkeit, Sitzungszustandsmaschine, Spuren, gebündelte Abfragen, TURN-TCP, Warmstart | Relay-Budgets erfüllt | K5 (≈16 %) |
| **S6 Doku als Spezifikation** | `ctox-rxdb.md` neu: Vertrag, Architektur, Protokoll; Einzelfälle ins Änderungsprotokoll | Doku = Vertrag | — |

Begründung: S1 ist billig, mechanisch und nimmt die größten Konfliktquellen weg (wirkt
sofort gegen U5). S2 und S4 hängen zusammen (das Profil legt fest, wer je Sammlung schreibt)
und laufen nach Strangler Sammlung für Sammlung; S3 liegt daneben, weil Autorität und
Eigentum Deklarationen im selben Vertrag sind. S5 zuletzt, weil Transport heute nur ~7 % der
Fixes verursacht und von weniger Daten (S4) profitiert.

IndexedDB-Bestände werden bei Vertragswechsel verworfen und neu geladen — erst nachdem lokal
gestufte, noch nicht synchronisierte Schreibvorgänge abgeflossen sind.

## 6. Arbeitsweise und Durchsetzung (aktiv seit 10.10.2026)

- **Ruleset auf `main`** („main: CTOX Sync v3 guard“): nur per PR, Pflicht-Check
  `sync-scope-guard`, kein Force-Push, kein Löschen, keine Ausnahmen. Alle Agenten pushen
  über dasselbe Admin-Konto; deshalb erzwingt nur das Ruleset, nicht CODEOWNERS.
- **`sync-scope-guard`**: PRs auf Sync-Kern-Pfaden brauchen ein Label `sync-v3:S<n>`, das
  der Owner vergibt. Offene Entscheidung: `store.rs` (Fix-Hotspot) aufnehmen, sobald S2 die
  Sync-Teile daraus herausgelöst hat.
- **Ein Umsetzungsstrang** im Sync-Kern; andere Stränge (Crew-Cockpit-Projektionen, Workjet
  Actions, Supervisor-Journale) melden neue Sammlungen mit Profil nach §3.1 an und legen
  keine neuen Projektionsketten ohne S4-Eintrag an.
- **Keine App-Workarounds** für Sync-Symptome; bestehende (Outbound-Zwischenspeicher,
  App-Readiness) werden in S2 zurückgebaut.
- **Feldbefunde** werden Benchmark-Fall + reproduzierender Test, kein neuer Doku-Abschnitt.
- **Fortschrittswache** alle 2 Stunden (geplante Aufgabe `ctox-sync-v3-wache`) prüft Stufe,
  Commits ohne Label, Stillstand des Umsetzungsstrangs und greift über den Supervisor ein.

## 7. Laufende Arbeiten und Zusammenführung

| Arbeit | Bezug | Vorgehen |
|---|---|---|
| Codex „Architektur Rework“ (6 Etappen: gemeinsamer Sync-Kern Workjet/CTOX) | S2–S5 | wird auf diese RFC ausgerichtet; seine Etappen werden den Stufen zugeordnet |
| Crew-Cockpit-Projektionen, Supervisor-SDK-Journale (#570, #578) | K1/S4 | keine weiteren Projektionswege ohne S4 |
| ctox#576 main-CI-Reparatur (SQLITE_SCHEMA in Katalogprojektion) | S0 | Label `sync-v3:S0`; Lesefehler 17 als wiederholbar behandeln (Statement neu vorbereiten), keine neue Projektion |
| PRs #85, #71, #31, #60, #481, #476 | S2/S4 | Owner ordnet je PR eine Stufe zu oder stellt zurück |
| Workjet Actions (neue Sammlungen Build-Knoten, Jobs, Leases) | §3.1 | Profile `control`/`stream`/`demand` von Beginn an |
| Präzedenzfälle | S4/§3.6 | Knowledge-Stream (Parquet on demand), Demand-Sidecar-Fix #243 |

## 8. Offene Entscheidungen

- Aufbewahrungsfrist abgeschlossener Befehle im Browser (Vorschlag 7 Tage).
- TURN über TCP/TLS 443 für alle Tenants (Kosten Cloudflare-TURN).
- `store.rs` in den Guard-Pfad aufnehmen (nach S2).
- Zeitrahmen: S0 1 Woche, S1 1 Woche, S2 3 Wochen, S3 2 Wochen (parallel zu S4-Beginn),
  S4 3–4 Wochen, S5 2–3 Wochen, S6 begleitend.
