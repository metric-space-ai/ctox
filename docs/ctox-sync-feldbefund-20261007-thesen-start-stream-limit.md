# Feldbefund 07.10.2026 — Shell-Start überrennt das Query-Stream-Limit (thesen)

Mandant: THESEN on-prem (srvki1), Release native-main-ecd15981a9cb, Shell-Stempel
`20261007-shell-v2-workjet-logo`, Outbound-Modul 1.0.302. Beobachtet am
Business-OS-Desktop über `ctox business-os serve` (Loopback-Sitzung, Rolle admin).

## Klasse

Beim Öffnen der Shell (und jeder App) startet der Browser deutlich mehr
gleichzeitige `rxdb.query.fetch`-Streams, als der native Peer zulässt
(`CTOX_QUERY_MAX_IN_FLIGHT_STREAMS = 8`, `query_fetch_handler.rs:680-693`).
Alles darüber hinaus wird mit `STREAM_LIMIT_EXCEEDED: max in-flight query streams
reached` (retryable) abgewiesen; der Client wiederholt mit Wartezeit
(`demand-loading-transport.mjs:437-441`). Ergebnis: Die Outbound-App braucht
nach einem Seitenaufruf rund 20 s bis zu den ersten Kampagnen/Leads, obwohl
die Einzelabfragen in 0,2–1,5 s beantwortet werden.

## Belege (Browser-Konsole, ein Seitenaufruf, 10 s)

- Über 3.000 `[V1.5]`-Debug-Meldungen in 10 s (Puffer lief über).
- Gleichzeitig `fetch:start` für `business_chats` (viele Fingerprints, je
  `limit: 1`), `business_commands` (`limit: 41`, `120`), `ctox_queue_tasks`
  (`limit: 39`, `41`, `120`), `outbound_lead_generation_research_policies`
  (derselbe Fingerprint 3× hintereinander `stale-served` + `start`),
  `outbound_lead_generation_leads` (derselbe Fingerprint
  `3851990b…` 3× `fetch:start`).
- Mehrfach `fetch:error … STREAM_LIMIT_EXCEEDED: max in-flight query streams
  reached` für `business_commands`, `ctox_queue_tasks`, `business_chats`,
  `research_policies`.
- `fetch:cancel … QUERY_CANCELLED: masterChangesSince failed for
  wor…fers: no master handler registered for collection` (vermutlich
  `workjet_session_transfers`; der Server meldet beim Start „skipping optional
  Business OS RxDB collection `workjet_session_transfers` (registration
  failed …)“).
- Erste Lead-Liste: `fetch:ok … docs: 200, ms: 1454` — die Abfrage selbst
  ist schnell, die Wartezeit entsteht in Ablehnung und Wiederholung.

## Vermutete Ursachen (nicht verifiziert)

1. Kein clientseitiges Zulassungslimit: der Demand-Loader startet alle Jobs
   sofort, statt höchstens `CTOX_QUERY_MAX_IN_FLIGHT_STREAMS` gleichzeitig zu
   fahren und den Rest zu warten (Warteschlange statt Fehler+Backoff).
2. Strikte Lesungen mit eigenem `requireRevision` (z. B. Outbound
   `loadLeadList`/`loadFullLeadRows`) haben einen eigenen `dedupKey`
   (`query-demand-loader.mjs:294-296`) und werden deshalb nicht mit identischen
   Fenstern zusammengelegt — derselbe Fingerprint läuft mehrfach parallel.
3. Das Debug-Logging `[V1.5]` ist im Produktivbetrieb aktiv und schreibt
   tausende Meldungen pro Seitenaufruf.

## Notfixes in dieser Sitzung (Symptom, nicht Klasse)

- `06b2d113c`, `fa1472f7c` (Outbound 1.0.300/1.0.301): Eine Sammlung, die in
  der Sitzung noch nie geladen wurde, zeigt 60 s lang „wird geladen“ statt
  „konnten nicht geladen werden … Neu verbinden“. Owner-Vorgabe: keine
  Fehlerbanner, wenn nichts kaputt ist.
- `eb41b7f57` (1.0.302): Fehlerursachen der Nachlade-Warnung als Text.

## Offen für den Sync-Refactoring-Worker

- Client-Zulassung auf das native Stream-Limit (Warteschlange), Messung
  „Zeit bis erste Lead-Liste“ vorher/nachher auf thesen.
- Zusammenlegen identischer strikter Fenster, solange eines unterwegs ist.
- `[V1.5]`-Debug-Logging im Produktivbetrieb abschalten.
- `workjet_session_transfers`-Registrierung auf thesen klären (cancel-Fehler).

## Quellstand der Architekturkorrektur

Auf main `a963dd343` existiert bereits eine pro Browser-Realm gemeinsame,
begrenzte Zulassungswarteschlange (bisher sechs aktive Streams). Die
`fetch:start`-Meldung entsteht vor dieser Zulassung und beweist daher allein
keinen gleichzeitig laufenden nativen Stream. Der native Grenzwert von acht
Streams gilt außerdem über alle Verbindungen; konkurrierende Browser können
weiterhin das serverseitige Limit erreichen.

Die Korrektur übernimmt den generierten Grenzwert acht in diese bestehende
Warteschlange, teilt identische laufende strikte Fenster im gemeinsamen Transport
und entfernt das standardmäßige `[V1.5]`-Logging. Berechtigungsdigest,
Verbindungsgeneration und jeder einzelne Abbruch bleiben getrennt abgesichert;
abgeschlossene Antworten werden nicht als neue strikte Lesung wiederverwendet.
Der gezielte Regressionstest misst Komponentenverhalten, keine THESEN-Latenz.

Die Vorher-Messung bleibt der obige Feldbefund (~20 s, identifizierter Release).
Die Nachher-Messung muss nach Claudes Installation am echten Outbound erfolgen:
Release/Shell-Stempel, Seitenaufruf bis erste sichtbare Lead-Liste, tatsächliche
RPCs/Limitablehnungen und Konsole erfassen. Ein lokaler Testlauf ersetzt sie nicht.

## Nachtrag 07.10.2026 17:10 (Claude, gemessen auf THESEN nach Installation)

Gemessen im Business-OS-Desktop vom Owner-Mac (WebRTC über TURN-Relay, daher
langsamer als im THESEN-LAN), Kennzahl „Seitenaufruf bis Kampagnenliste sichtbar“.

| Stand | Zeit |
|---|---|
| native-main-ecd15981a9cb, Outbound 1.0.302 | 22,6 s / 24,1 s |
| + 86098f681 (Slot-Warten im Transport, `[V1.5]` nur mit `__CTOX_V15_DEBUG__`) | 27–33 s (keine Besserung) |
| + 0fc6e7176 (Limit-Ablehnung über die RPC-Antwort wird wiederholt) | 27 s |
| + Outbound 1.0.303 d3852fcc5 (gleichzeitige Lead-Listen-Ladungen geteilt, 45 s Budget) | 21,6 s; 0 `fetch:error`, keine Doppelseiten |

Zusätzlich gefundene Ursache (in 0fc6e7176 behoben): Der native Peer lehnt eine
Abfrage über dem Limit zweimal ab — als RPC-Antwort ohne `retryable` und als
`rxdb.query.error`-Frame. Die Antwort kommt zuerst; der Client verlangte
`retryable` und wiederholte deshalb nie. Der neue Smoke scheitert gegen den
alten Transport genau damit.

Verbleibende Zeit (Messlauf 15:06Z, Outbound 1.0.303):
- 0–12,5 s: noch keine einzige Abfrage — Peer-Verbindungsaufbau vom Mac.
- ab 12,5 s belegen Shell-Abfragen 5 von 6 Client-Slots für 9–11 s:
  `ctox_queue_tasks` (limit 120 und 200), `business_commands` (200),
  `ctox_harness_events` (198), `ctox_runs` (197). Die Leads der geöffneten App
  bekommen den letzten Slot; 7 Seiten à 200 nacheinander, je 1,4–1,8 s.
- Die Kampagnenliste wird erst bei `fertig` sichtbar, nicht nach dem ersten
  vollständigen Laden (~23 s).
- Nach `fertig` lädt Outbound die ganze Lead-Liste fast im Sekundentakt neu
  (laufende Recherche-Writebacks invalidieren die Sammlung) — Dauerlast.

Vorschlag: Abfragen der sichtbaren App vor Hintergrund-Abfragen der Shell
zulassen (Priorität in der Zulassungswarteschlange) und die großen
Shell-Fenster beim Start verkleinern oder verzögern.
