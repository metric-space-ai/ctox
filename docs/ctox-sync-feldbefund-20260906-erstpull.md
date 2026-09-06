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
