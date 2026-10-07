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
