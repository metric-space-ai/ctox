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


### 5.1 S1-Gate-Plan: Modulgraph und Inventare (11.10.2026)

**Plan zur Gate-Prüfung, vor der ersten S1-Code-Scheibe.** S0-Werkzeuge aus
#585/#597/#598/#600 sind auf main. S1 läuft unabhängig vom noch offenen
kontinuierlichen 24-h-Nachweis. Diese Scheibe ändert ausschließlich die RFC;
die folgenden Grenzen sind vorgeschlagene S1-Regressionsbudgets, keine neuen
Messungen und keine bereits erfüllten Produktziele.

#### Lieferumfang und Reihenfolge

| Scheibe | Ergebnis | Gate |
|---|---|---|
| S1-A Modulgraph erzeugen | Deterministisches Manifest der Shell-, Sync- und App-Einstiege einschließlich transitiver ESM-, Worker-, CSS- und sonstiger Asset-Abhängigkeiten; bestehende Loader zunächst weiter bedienen. | Zwei saubere Generierungen ergeben dieselben Bytes; keine Zeit-, Pfad-, Host- oder Zufallsanteile. Fehlende Abhängigkeit, mehrdeutiger Einstieg oder nicht deklarierter dynamischer Import scheitert mit Quellort. |
| S1-B Modulgraph konsumieren | Import-Map bzw. generierte URLs aus Inhaltshashes; ein stabiler Bootstrap wählt genau ein Manifest einer veröffentlichten Shell-/Modulrevision. Alle bisherigen globalen `?v=`-/Generationsstempel im deklarierten Umfang entfallen. | Warm-/Kaltladen, Update und Rollback laden jeweils einen vollständigen Graphen; Änderung eines Blatts invalidiert genau seine betroffenen transitiven Verbraucher. Unabhängige Teilgraphen behalten ihre Identität. Kein Commit ist mehr allein zum Stempelwechsel nötig. |
| S1-C Kanonisches Register | Ein Register für Befehle, Sammlungen, Schemata und deklarierte Verbraucher; daraus native/browserseitige Inventare, statische Allowlisten, Schemadarstellungen und menschenlesbare Übersicht erzeugen. | Jeder bisherige Eintrag hat Quelle, Verbraucher und Migrationsstatus; fehlende, doppelte oder widersprüchliche IDs/Referenzen scheitern. Beide Verbraucher werden gemeinsam regeneriert. Kein zweites handgepflegtes Inventar. |
| S1-D Verbraucher und Release-Prüfung | Bestehende Verbraucher schrittweise auf die erzeugten Artefakte umstellen; driftfreie Regenerierung in der vorhandenen Release-/CI-Prüfung erzwingen. | „generiert = eingecheckt“ auf dem finalen Head; absichtlich veränderte Ausgabe schlägt fehl. Bestehende Wire-, Schema-, Rechte- und Shell-Gates bleiben wirksam. S0-Vorher/Nachher im selben PR. |

Jede Scheibe bleibt ein PR von höchstens 70 Minuten Implementierungsarbeit mit
`sync-v3:S1` vom RFC-Owner. Scope, betroffene Verbraucher und noch verbleibende
Legacy-Einstiege stehen im PR. S1 endet erst mit vollständiger Abdeckung des
deklarierten Graphen und Registers; ein teilweise generiertes Verzeichnis
erfüllt das Abschlussgate nicht.

**Modulgraph-Vertrag.** Ein Knoten bindet kanonischen Quellpfad, exakte
Asset-Bytes und deklarierte Abhängigkeiten; die Identität bindet auch deren
Inhalte. Zyklen werden als deterministisch geordnete Komponenten behandelt,
nicht über rekursive, voneinander abhängige Hashes. Der Generator unterscheidet
Quellhash, erzeugten Asset-Hash und Manifest-Hash. Nicht statisch auflösbare
Importziele brauchen eine endliche deklarierte Menge oder einen ausdrücklich
versionierten Erweiterungseinstieg; sie werden nie still ausgelassen.
Reihenfolge, absoluter Build-Pfad und Dateizeit dürfen keine Identität ändern.
Eine Byteänderung, eine entfernte Abhängigkeit und ein Zyklus sind negative
bzw. Perturbations-Fixtures. Unbenutzte Assets werden als solche ausgewiesen.

Für runtime-installierte Module wird der Graph aus der jeweiligen zugelassenen
Modulrevision erzeugt und an sie gebunden; die Installation erfordert keinen
neuen nativen Compilerlauf. Shell-Pack und Rollback referenzieren vorhandene
vollständige Graphen. Manifestintegrität ersetzt weder Paket-Signatur noch
Installations-/Source-View-/Datenrechte. Schema- oder Storage-Generationen sind
keine Asset-Stempel: ihre Lebenszyklussemantik bleibt bis zur jeweiligen
Vertragsstufe erhalten. Dieser Wechsel löscht weder IndexedDB noch lokale
ausstehende Writes.

**Register-Vertrag.** Die bestehende kanonische Wire-Fixture-Pipeline
(`docs/ctox-rxdb.md`, §7) bleibt erhalten und wird eingebunden; bereits
kanonische Fixtures werden nicht als zweite Quelle kopiert. Das Register
verknüpft Befehls-ID/Version, Control-/Queue-Klassifikation, Payload-/Ergebnis-
Schema, Handler, Sammlungen, Migrationen, Schreiber, Leseverbraucher und
statische Zugangsdeklarationen. Dynamische Modulbefehle behalten den bestehenden
nativen, policy-geprüften Erweiterungspfad; ein statisches Inventar darf sie
nicht pauschal verweigern oder freigeben.

Native und Browser werden aus derselben Schemaquelle versorgt. Jede Sammlung
führt ihre nativen Import-/Projektions-/sonstigen Schreibpfade mit
Schema-Version und Validierungspunkt auf, insbesondere `outbound_lead_import`.
Fehlende Validierung oder mehrfache Schreiber erscheinen als offene S2/S4-Lücke,
nicht als erfundener erfolgreicher Nachweis. Noch nicht festgelegte Profile,
Eigentümer und Feldauthorität bleiben ausdrücklich unbekannt mit zuständiger
Stufe. S1 erzeugt keine Grants, erweitert keine Rechte und ändert keine
Command-/ACK-Semantik; die Zuordnung wird durch vorhandene Policy-Tests und
Vorher/Nachher-Inventardiffs geprüft. Geheimnisse, Tokens, Credential-Referenzen
und private Kontodaten gehören in keines der veröffentlichten Artefakte.

#### S0-Bezug und Messdefinitionen

Die unveränderte Scale-Fixture hat **9.250 Dokumente / 257.848.000 JSON-Bytes**:
850 Leads, 6.000 Befehle, 400 Queue-Tasks und 2.000 Chats als isolierte
größenäquivalente Modulsammlungen. Sichtbar werden 20 vollständige 80-KB-Zeilen;
je Fall gibt es fünf sequenzielle Writes mit echter Master-ACK und unabhängigem
SQLite-Readback. Gemessen wird über tatsächliche RTCPeerConnections/DataChannels
mit UDP-Relay **0/300/600 ms RTT**, halber Verzögerung je Richtung und geprüfter
Delay-Oracle. Das ist keine installierte TURN- oder Outbound-Abnahme.

| Baseline | Werkzeug-Head / gpu3-Lauf | Sichtbar 0/300/600 ms RTT (ms) | ACK-p95 0/300/600 ms RTT (ms) |
|---|---|---|---|
| S0-2 (#597) | `355af7aa16ea55b1ac3f8f16911c1eedc640ed31` / `sync-v3-s0-relay-latency-20261010T174613Z` | 5.185,9 / 17.455,2 / 35.775,3 | 205,4 / 4.067,5 / 7.963,7 |
| S0-3 (#598) | `03a843f80d514dff86eef6aad1aba69f33ee3879` / `sync-v3-s0-phase-rtts-20261010T194728Z` | 5.245,4 / 19.283,2 / 34.973,6 | 236,2 / 4.444,4 / 7.954,9 |
| S0-4 (#600) | `6bb64615f921993285e14c49dba5f0b20467d9f6` / `sync-v3-s0-resources-20261010T203520Z` | 3.279,4 / 19.389,7 / 38.069,5 | 220,3 / 5.092,5 / 9.234,6 |

Alle drei verwenden den shared native `7e21f6052b633d5efaa85e8c5d6c1b420d527076`,
Binary-SHA256 `741b7260925c1e27b11cc8100d7877f1f582325523f6d31d1a69e4dac4c4f8ba`.
S0-3 nennt hier **Navigation start → sichtbar**; sein Observer-Boot beginnt
später. Beim Vergleich bleibt die Messdefinition pro Paar identisch.
Statische HTTP-Assets liegen außerhalb der Relay-Verzögerung. S1 misst deren
Requests/Cache-Hits/Bytes zusätzlich separat und schreibt sie nicht den
DataChannel-Zählern zu. Deren S0-Baseline ist bislang **unbekannt**; S1-A erhebt
sie vor der Loader-Umstellung.

#### Budgets je S1-Prüfpunkt

Die S0-Läufe sind explorative Einzelmessungen, keine statistischen SLOs.
Für den mechanischen S1-Umbau gilt: Vorher und Nachher mit denselben Werkzeugen,
Fixture, Browser-Version, Ressourcenlimits und Relay-Fällen auf der gpu3-Lane.
Der tatsächliche Native-Head und Binary-Digest stehen in beiden Belegen; bei
einer nativen Änderung muss auch der Nachher-Binary diesen Head enthalten.
Die historischen Werte werden niemals als Messung eines neuen main relabelt.

Für Zeit/Heap/IDB/CPU gilt sowohl die untenstehende historische Hülle als auch
**höchstens +10 % gegenüber dem passenden Vorher-Lauf**, zuzüglich nur der
ausgewiesenen Messauflösung (CPU: 20 ms bei 100-Hz-/proc-Ablesung).
Die Hülle ist 110 % des höchsten passenden S0-Wertes, nach oben gerundet.
Ein Überschreiten bleibt ein Finding; keine selektive Mittelung oder Änderung
von Fixture/Timeout, um es zu verstecken. Nur ein konkreter Mess-/Umgebungsfehler
begründet eine Wiederholung mit erhaltenem ersten Ergebnis.

| Prüfpunkt | S0-Ist 0/300/600 ms RTT | vorgeschlagenes S1-Budget / Nachweis |
|---|---|---|
| Graph/Register deterministisch | vorher von Hand gepflegte Stempel/Inventare | 0 Byte Drift bei zwei Generierungen; 0 undefinierte Referenzen; 0 still ausgelassene Importe; 0 zusätzliche manuell gepflegte Vertragskopien. |
| Cache-/Update-/Rollback-Kohärenz | globaler Generationswechsel, Asset-Bytes bisher unbekannt | 0 gemischte Graphrevisionen, 0 stale/missing Assets; Änderung bleibt im betroffenen Teilgraphen. Unverändertes Warm-Reload lädt 0 neue gehashte Asset-Payloads. Bootstrap-Revalidierung separat zählen. Asset-Payload/Request-Zahl im gleichen Kaltfall steigt gegenüber S1-A um höchstens 10 %; keine zusätzliche sequenzielle Loader-Rundreise. |
| Navigation → sichtbare Daten | Hülle 5.245,4 / 19.389,7 / 38.069,5 ms | ≤ 5.770 / 21.329 / 41.877 ms, zusätzlich Paar-Regel. Keine Readiness-Abkürzung oder leere Liste als Erfolg. |
| Write → akzeptierter Master-ACK, p95 | Hülle 236,2 / 5.092,5 / 9.234,6 ms | ≤ 260 / 5.602 / 10.159 ms, zusätzlich Paar-Regel; 5/5 gültige Writes je Fall im nativen SQLite bestätigt. Transport-ACK allein zählt nicht. |
| Browser-Heap peak | S0-4: 28,97 / 25,39 / 38,43 MiB | ≤ 31,87 / 27,94 / 42,28 MiB, zusätzlich Paar-Regel; dieselben CDP-Samples, kein erzwungener GC. |
| IndexedDB am Ende | S0-4: 5,104 / 5,340 / 5,604 MiB | ≤ 5,62 / 5,88 / 6,17 MiB, zusätzlich Paar-Regel; kein Vollreplikat der 257,8 MB im Demand-Cache. 20-s-Idle-Zuwachs ≤ 8 KiB (S0: 4.860 / 4.146 / 0 B); ausstehende Writes bleiben erhalten. |
| Native Server-CPU gesamt | S0-4: 2.940 / 4.520 / 5.050 ms | ≤ 3.254 / 4.992 / 5.575 ms einschließlich 20-ms-Auflösung; zusätzlich Paar-Regel. |
| Native CPU je Phase | S0-4: Boot 310/470/450; Health 2.090/3.310/3.610; Setup 0/0/0; Readiness 30/0/80; Query 210/330/420; Paint 0/0/0; fünf Writes 140/350/440 ms | Je genannter Phase ≤ Ist × 1,10 + 20 ms und Paar-Regel. Kein Summieren überlappender Thread-Envelope-Werte; unbekannte Kurzzeit-Threads bleiben unbekannt. |
| Protokoll-/RTT-Aufschlüsselung | S0-3: 35,57 Health- und 13,67 Query-RTT-Äquivalente; Write 2 + 4 sequenzielle Frame-ACK-Fenster | 0 durch S1 zusätzlich eingeführte sequenzielle Protokoll-/ACK-Barrieren. Dieselben Phasen ausweisen; parallele RPCs und pipelinierte ACKs nicht als sequenzielle RTTs zählen. |
| Fehler-/Schema-/Rechte-Gates | unabhängige Byte-/Delay-Oracle, vorhandene Drift- und Policy-Tests | 0 gefälschte ACKs, übersprungene Pflichtprüfungen oder Rechteerweiterungen; Negative Fixtures müssen fehlschlagen. Byte-Oracle bleibt exakt, nicht mit 10-%-Toleranz. |

Die S0-4-Heapwerte enthalten Beobachter und keine GC-Normalisierung; IDB ist
Origin-Quota, keine gesamte Browser-RAM-Messung. CPU ist der vollständige
eigene native PID, nicht ein bewiesener Projektionspass. Projektionspass-CPU
und Schreibverstärkung fehlen noch als gesonderte Baselines für S2/S4 und
werden nicht aus der Health-Phase erfunden. Bei Bytes bleiben SCTP-Anwendungs-
Bytes, ICE-Statistik und UDP-Datagramm-Bytes getrennt; das ≤2-MB-Produktziel aus
§4 wird nicht mit einem anderen Zähler für bestanden erklärt.

**S5 bleibt verbindlich:** bei 600 ms RTT Navigation bis sichtbare Daten
≤ **10 RTT-Äquivalente (6.000 ms)**, Write-ACK-p95 ≤ **3 RTT (1.800 ms)**.
Diese End-to-End-Ziele sind keine Differenz-von-0-ms-Berechnung; S0-3 verwendet
diese Differenz nur zur Ursachenanalyse. S0/S1 erfüllen die S5-Ziele bislang
nicht. Die höheren S1-Regressionshüllen erlauben keine Lockerung von §4/S5.

#### Fester Benchmark-Fall: 422 darf keine Sammlung blockieren

Fall-ID **schema-rejected-write-readiness**. Ausgangsbefund: nativer
`outbound_lead_import` ohne `contacts`/`evidence`, danach permanenter
`master_write`-Fehler 422, endloser Push und Readiness-Timeout 44.998 ms;
laut THESEN-Bericht 6/6 fehlgeschlagene App-Starts. Die genaue Feldrevision
und Dokument-IDs sind nicht geliefert; das ist ein gemeldeter Feldbefund,
noch keine unabhängig reproduzierte Fixture.

Die kommende Fixture verwendet das echte kanonische Lead-Schema und dieselbe
Scale-Verteilung, aktuelle native Autorität und den echten WebRTC-Schreibpfad.
Zwei Varianten lassen jeweils `contacts` bzw. `evidence` weg. Ein ausschließlich
offline erzeugter, isolierter Altbestand enthält das fehlerhafte Dokument,
um die Bestands-Reconciliation zu prüfen; die normale Produktion darf solche
Zeilen nicht als Testinjektion schreiben. Parallel liegen gültige sichtbare
Zeilen und ein gültiger unabhängiger Write vor. Ein Kontrolllauf ohne die
ungültige Zeile verwendet denselben Seed und dieselben IDs. Alle drei
Relay-Fälle, Reload und korrigierte gezielte Wiederzustellung gehören zum Fall.

| Stufe | Prüfpunkt und Erwartung |
|---|---|
| S1 | Register ordnet Import und Browser-Reconciliation demselben Schema zu; fehlende native Vorabvalidierung ist als S2-Lücke sichtbar. Reproduktion zählt tatsächliche Requests/Rejections, Zeitpunkt der Bereitschaft und Quarantäne-Status. Der aktuelle Defekt bleibt als erwartetes negatives Baseline-Ergebnis erhalten; S1 wird dadurch nicht fälschlich als repariert gemeldet. |
| S2 | Derselbe ungültige Import wird vor Persistierung gegen das kanonische App-Schema abgelehnt; keine gültige Dokumentrevision entsteht. Beide Pflichtfeldvarianten und ein gültiger Import werden geprüft. |
| S5 | Ein permanenter Schemafehler beendet automatische Zustellung genau des betroffenen Writes. Sichtbarer, dem berechtigten Nutzer zugeordneter Quarantäne-Eintrag mit Dokument-/Write-ID, Fehlerklasse, Schema-Version und Korrekturaktion; ausstehende Änderungen bleiben erhalten. Kein erfolgreicher ACK für den abgelehnten Write. |
| S5 | Gültige Zeilen erreichen query-spezifische Bereitschaft trotz Quarantäne: Zeit ≤ Kontrolllauf × 1,10 + 1.000 ms, zusätzlich innerhalb der S5-Relay-Ziele. Der unabhängige gültige Write bekommt einen echten akzeptierten ACK und SQLite-Readback. 0 automatische Wiederholungen nach klassifiziertem permanentem Fehler, auch über Reload; nur ausdrückliche Korrektur/erneute Zustellung reaktiviert den Write. |
| S2/S5 | Quarantäne wird nach Reload sichtbar wiederhergestellt; korrigierter Write wird genau einmal erfolgreich zugestellt und sein Eintrag entsprechend aufgelöst. Authority-/Transportfehler bleiben getrennt und behalten ihre bisherigen sicheren Retry-/Revocation-Regeln. Keine fremden IDs/Payloads werden zur Diagnose offengelegt. |

Quarantäne-Repräsentation und UX werden in S2/S5 spezifiziert, ohne neuen
Projektionsschreiber oder HTTP-Datenpfad. Der Benchmark darf die fehlerhafte
Zeile weder still löschen noch sie als gültig ausgeben oder Readiness
synthetisch bestätigen.

#### Nachweis und offener S0-Abschluss

Jeder Code-PR enthält Generator-/Consumer-Head, tatsächlich verwendeten Native-
und Browser-Stand, Lane-ID, Rohbericht, Zähler-Selbsttests, Vorher/Nachher samt
Abweichungen und Cleanup. Linux-/Browserläufe gehen über die normale gpu3-Lane
in eine isolierte Acceptance-Präfix; vorhandene shared Binaries werden nur
wiederverwendet, wenn sie den zu prüfenden nativen Stand tatsächlich abdecken.
Es gibt keinen parallelen Baseline-Build, Tenant-Deploy oder Live-Faultlauf.

Der erste S0-4-Soak wurde am **10.10.2026, 22:24:19Z** für P0 normal beendet:
6.145,282 s, 1.400 Writes, 1.049 Probes, 0 geloggte Busy-Ereignisse/Probe-Retries,
maximale Probe-Admission 27,809 ms. **Kein 24-h-Pass und kein laufender Soak**
aus diesem Beleg. Ein neuer kontinuierlicher 24-h-Lauf braucht eigene Unit und
Ergebnisdatei sowie normale Lane-Admission; Intervalle werden nicht addiert.
Das S0-Abschlussbudget bleibt 0 geloggte Lockfehler/0 fehlgeschlagene Writes,
0 abgewiesene Probes und 0 Probe-Busy-Retries über volle 24 h. Unprotokollierte
native Busy-Handler-Wartezeit bleibt unbekannt. Dieser Nachweis läuft separat
vom S1-Gate und hält S1 nicht auf.

Die Ausgangsbelege liegen beim Umsetzungsowner unter
`~/.codex/task-evidence/sync-v3/`: `S0-2-final-report.json`,
`S0-3-final-report.json`, `S0-4-METRICS.json`,
`S0-4-P0-YIELD.json` und `S1-S2-INVENTAR-20261010.md`.
Die jeweiligen PRs #597/#598/#600 bewahren die Lane-Zuordnung und
Messlimits. Dieser Plan-PR führt keinen neuen Messlauf aus.

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
