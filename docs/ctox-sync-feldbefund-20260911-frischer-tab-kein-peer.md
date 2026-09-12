# Feldbefund 11.09.2026 — frischer Tab: erster Befehl ohne authentifizierten Peer

Instanz: thesen (managed tenant), Business OS über Loopback-Tunnel, Rolle admin.
Quelle: Klicktest der Outbound-Lead-Generation-App, Bereich P1
(`thesen-operations/2026-09-11-outbound-ui-test/runs/P1/results.md`, Befund B1).

## Beobachtung

- In **2 von 5** frisch geöffneten Tabs scheiterte der erste Befehl der App
  (`outbound.research_source.registry_read`, `ctox.secret.list`,
  `outbound.sellify.lookup`) nach 45 s mit
  `CTOX Sync Engine collection "business_commands" has no authenticated WebRTC
  peer after 45000 ms (…)` aus `shared/command-bus.js` (Wartepfad vor dem
  lokalen Insert, `waitForSyncBridgeReady`).
- Auf dem Server erscheint dafür **keine** Zeile in `business_commands` —
  der Befehl wurde nie eingefügt, nicht verloren.
- Ein zweiter Versuch derselben Aktion im selben Tab gelang meist sofort.
- Randbedingung: vier parallele Playwright-Sitzungen (P1–P4) gegen dieselbe
  Instanz, dazu laufende Recherche-Worker; Server-Peer meldete durchgehend
  `replicationUp: true`, andere Clients schrieben normal.
- In P4 (T39/T40) zusätzlich im Browser `QUERY_CANCELLED: peer-not-open` und
  `replication-cancel`; ein lokaler `incrementalPatch` einer Quelle kam in
  diesem Tab nie am Server an, bis ein frischer Tab geöffnet wurde.

## Folgen in der App

- Vor 1.0.167 schluckte die App diese Fehler (`console.warn`); das
  Quellen-Panel zeigte dann „Zugang fehlt“ für Quellen mit hinterlegtem
  Zugang. App-seitig abgefangen ab 1.0.167 (Stand „unbekannt“, einmaliges
  Nachfassen nach 15 s, zentrale Fehlermeldung für Klick-Aktionen).
- Der Grundfehler — ein frischer Tab hat bis zu 45 s keinen authentifizierten
  Peer für `business_commands` — liegt in der Sync-Engine bzw. im
  Peer-Handshake und ist mit App-Mitteln nicht zu beheben.

## Nachstellung

Vier parallele Sitzungen öffnen `#outbound-lead-generation`, sofort nach
Ende der Synchronisation das Quellen-Panel (Zahnrad). Zählen, wie viele
`registry_read`-Befehle in `business_commands` ankommen. Erwartet: alle.

## Offene Frage an die Sync-Engine

Warum dauert die Peer-Authentifizierung eines neuen Tabs unter Last länger
als 45 s, obwohl der Server-Peer gesund ist, und warum erholt sich ein Tab mit
`peer-not-open` nicht selbst?

## Nachtrag 11.09.2026 abends — zweiter Schreibvorgang kurz nach dem Einfügen geht verloren

Quelle: Nachtest F (`thesen-operations/2026-09-11-outbound-ui-test/runs/F/retest-final.md`, NF-2).

- Outbound 1.0.172 fügte eine Quelle ein (`sources.insert`, `auth_status=required`) und
  patchte sie ≈1 s später (`incrementalPatch({auth_status:'credential_available'})`).
- Der Server blieb 70 s lang und auch nach dem Schließen des Tabs auf Revision
  `1-…` mit `auth_status=required`. Der zweite Schreibvorgang kam nie an, und im
  Browser erschien keine Konsolenmeldung.
- Das ist dieselbe Klasse wie der Befund N1 aus Stufe 3: ein Schreibvorgang kurz nach
  einem vorigen auf dasselbe Dokument geht verloren.
- Die App umgeht das ab 1.0.174 mit einem einzigen Schreibvorgang. Der
  Grundfehler (Push-Verlust bei schneller Folgeänderung) liegt in der
  Sync-Engine.

Nachstellung: im selben Tab `insert(doc)`, sofort danach `incrementalPatch`
auf dasselbe Dokument, dann die Serverrevision nach 30 s lesen. Erwartet `2-…`.
