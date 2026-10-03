# Sync-Feldbefund 27.09.2026: thesen — Abruf `ctox_queue_tasks` antwortet nicht

> **KORREKTUR 28.09.2026 06:13 UTC — kein thesen-Serverdefekt.** Mit einem frischen Headless-Chrome
> (eigener Kontext, ein Tab) über den SSH-Tunnel auf `ctox business-os serve` lädt die Crew-App auf
> thesen 98 Aufgaben in 28 s. Die Abrufe `ctox_queue_tasks` a0577fea… liefern `fetch:ok docs: 120, ms: 7827`.
> Der beobachtete Stillstand betraf nur das eingebettete Browser-Panel dieser Sitzung.
> - Das Panel teilt sein Profil mit anderen Sitzungen. Ein thesen-Tab einer anderen Sitzung hielt den
>   Web-Lock `ctox-rxdb-sync:…:11b37of` (Leader `b4386db7…`, `clientId F71FE67A…`).
> - Das Panel lief deshalb als Follower (`multiTab.role=follower`, `queryReady=false`) und bekam für
>   `ctox_queue_tasks` sofort ein leeres Fenster (0 Dokumente, 0 ms). Auf dem Broker-Kanal stand in 25 s
>   kein Anspruch, der Leader sendete aber `replicated-change` auch für `ctox_queue_tasks`.
>
> Offene Frage an das Refactoring bleibt: Warum bekommt ein Follower-Tab für eine demand-only-Collection
> dauerhaft ein leeres Fenster, statt selbst abzurufen oder den Leader zu fragen? Die Serverlast-Zahlen
> unten bleiben als Messung stehen.

Beobachtet von der Crew-UX-Kampagne bei der Browser-Abnahme von Shell v410 auf
thesen.ctox.dev. Übergabe an das Sync-Refactoring. Die Kampagne ändert daran nichts.

## Symptom

- Die Crew-App (`modules/ctox`) bleibt bei „Wird geladen…“ stehen, und alle Reiter zeigen 0.
- Die Browser-Konsole zeigt für `ctox_queue_tasks` nur `[V1.5] fetch:start` (Fingerprints
  `a0577fea…` mit limit 120, `5eabd534…`, `b681d0fe…` und `8f8cca9a…` mit limit 200).
  Über mehr als 8 Minuten kam weder `fetch:ok` noch ein Fehler.
- Andere Collections kommen durch, allerdings langsam: `ctox_crew_members` braucht 23,9–29,6 s,
  `ctox_runtime_settings` 11,5–14,0 s und `ctox_harness_status` 14,5–15,6 s.
  `ctox_harness_status` wird im Sekundentakt neu abgerufen: stale-served, dann ein neuer
  Start. Der Tab puffert über 20 000 Konsolenzeilen.
- Sync-Diagnose: `mode=webrtc`, `phase=collection-sync`, 31 Collections, 30 davon
  connected oder reused und 1 pending. `lastLifecycleEvent` meldet `peer_connect_timeout`
  für `business_workspace_branding`.

## Server (14:07–14:12 UTC)

- `ctox-real service` läuft mit 250 % CPU und 59 % RAM, der Load liegt bei 7,4. Parallel
  läuft ein Playwright-Chromium (interactive-reference). `ctox status`: running, nicht
  busy, 0 aktive Worker, 37 pending.
- Native Peer: `replicationUp=true`, Circuit closed, `business_commands` alive.
- `business-os-rxdb.sqlite3` ist 1,5 GB groß. Die Tabelle
  `ctox_business_os__ctox_queue_tasks__v3` enthält 580 Dokumente (216 mit `_deleted`) und
  insgesamt 13,7 MB. Ein Dokument hat im Mittel 23,7 KB, das größte 45,7 KB, keines ist
  größer als 64 KB.
- Ein Abruf mit limit 200 entspricht damit etwa 4,7 MB auf einer Seite.
- Im Journal gibt es keine Zeile zu `queue_tasks`. Dort stehen nur wiederholte Meldungen
  `skipping oversized knowledge item` (Skills mit 340–830 KB).

## Abgrenzung

- welsch zeigt mit identischem Abfragecode (v409, Slot beta.63) im selben Zeitraum 34
  Aufgaben.
- v410 ändert nur den Lade-/Fehlerplatzhalter (neutraler Crew-Geist) und den Stempel.
  Das Gate `assert-ctox-data-state.mjs` (Übergang Laden → Daten) ist mit v410 lokal grün.

## Offene Fragen an das Refactoring

1. Warum endet ein Demand-Fetch ohne `fetch:ok` und ohne Fehler? Fehlt ein Timeout, oder
   geht die Antwort verloren?
2. Ist die Seitengröße (etwa 4,7 MB bei limit 200) unter Serverlast der Auslöser?
3. Ist der Sekundentakt der Abrufe von `ctox_harness_status` gewollt?
