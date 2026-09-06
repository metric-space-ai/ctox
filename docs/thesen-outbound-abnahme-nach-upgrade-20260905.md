# Abnahme der Outbound-App nach dem Queue/Harness-Upgrade (05.09.2026)

Maßstab (Eigentümer, 05.09.): Die Fixes müssen darauf einzahlen, dass die
Outbound-App anschließend korrekt funktioniert. Grüne Tests sind Voraussetzung,
nicht Abnahme. Abgenommen wird auf `thesen.ctox.dev` im Browser und im
Routing-State (`communication_routing_state`), nie in der Projektion allein.

## Was jeder Befund in der App bewirken muss

| Befund | Sichtbares Verhalten in der App nach dem Upgrade | Messung |
|---|---|---|
| 1 Lease-Sweep | Stirbt ein Worker mitten in einer Recherche, fällt der Lead binnen ≤ 20 min von „Läuft" auf „Wartet" zurück und wird erneut gestartet. Kein Lead bleibt für immer auf „Läuft". | Routing-State: `leased` → `pending` nach `lease_expires_at`; Lead-Zeile in der App wechselt. Probe: laufende Recherche, Worker-Prozess beenden, 20 min warten. |
| 2 Reparatur-Entdopplung | Bei Quellenfehlern entsteht je Ziel höchstens EINE offene Reparatur; nach drei Fehlschlägen ist Schluss (`failed` mit Grund). Die Recherchen stehen nicht mehr hinter Dutzenden Reparaturen. | `ctox queue list --status pending` nach Titel gruppiert: kein Titel mehrfach. Über 24 h beobachten. |
| 3 Kapazität | Mit `ctox queue capacity --workers 4` laufen vier Recherchen gleichzeitig: „Auswahl recherchieren (8)" → vier Leads auf „Läuft", nicht einer. Eine 19-Firmen-Kampagne dauert Stunden, nicht einen Tag. | Routing-State: `count(route_status='leased')` ≥ 4 bei ≥ 8 wartenden. App-Zähler „Laufend". |
| 4 Projektion | Stornieren (CLI oder App) nimmt die Aufgabe sofort aus „Läuft"; die sieben Altlasten vom 28.–31.08. verschwinden; CTOX-Modul zeigt nur, was es gibt. | Projektion `ctox_queue_tasks` == Routing-State je `id`; Anzahl `status=running` in Projektion == `leased` im Routing-State. |
| 5 Writeback | Jede abgeschlossene Recherche schreibt zurück (Lead → „Prüfung nötig" mit Feldern) ODER der Task endet `failed` mit Grund. Nie mehr „erfolgreich" mit 0 Feldern. | Kein Lead mit Task `completed`/`handled` und `field_status`-Zählung 0. Chat zeigt den Grund bei Fehlschlag. |

## Was nicht schlechter werden darf (Regression)

- Klick „Auswahl nachrecherchieren" → Hinweis, Chatfenster, Lead auf „Wartet" (B1, 1.0.94–1.0.99)
- Personenwechsel im Lead-Detail zeigt verschiedene Inhalte
- Dialoge als Overlay in der App, nicht im Seitenfluss (1.0.90)
- Chateingabe bleibt beim Tippen stehen (c378aaf83)
- Kampagnenliste: eine Kampagne „Chemie", 19 Leads, 19 mit Ergebnis
- Schreibtisch lädt in < 90 s; keine Neustart-Schleife der Kollektionen

## Ablauf

1. Befund 1–4 auf `origin/main` (Befund 5 darf nachziehen) → ein Upgrade `ctox upgrade --dev`.
2. Nach dem Umschalten: `ctox queue capacity --workers 4` — **Achtung: der Code-Standard ist bereits 4** (`unwrap_or(4)` in `service_queue_capacity.rs`), nicht 1 wie zunächst angenommen. Parallelität greift also sofort nach dem Umschalten; das explizite Setzen macht sie zur dokumentierten Entscheidung. Erste Beobachtung direkt danach: welche vier Aufgaben werden geleast (Recherche, Auth-Assist, Reparatur?).
3. Regressionsliste im Browser durchklicken.
4. Befund 2/3/4 sofort messen (Routing-State + App); Befund 1 per Probe; Befund 5 an der nächsten realen Recherche.
5. Ergebnis mit Zahlen in dieses Dokument, dann Push auf `main`.

## Baseline VOR dem Upgrade (05.09.2026, 06:38 UTC, Release branch-main-20260904T161717Z)

| Messung | Wert vorher |
|---|---|
| Befund 3: Routing-State | 0 `leased`, 30 `pending` — Kapazitätsbefehl existiert nicht |
| Befund 4: Projektion `running` vs Routing-State `leased` | **7 vs 0** — sieben Phantome (alle im Routing-State `cancelled` seit 28.–31.08.) |
| Befund 2: doppelte offene Titel | **24 Dubletten** (19× evi-gv-at, 3× maps-google-com, 2× handelsregister-de) — vierte Welle; von Hand auf je eine reduziert (21 storniert, 9 offen) |
| Befund 5: Leads mit Ergebnis | 19 / 19 (`research_status`), keiner mit 0 Feldern |
| Befund 1: aktive Leases | keine zum Messzeitpunkt |

Erwartung nach dem Upgrade: Befund 4 → 0 Phantome (oder Reconciler räumt sie);
Befund 2 → über 24 h keine Dubletten mehr; Befund 3 → nach `capacity --workers 4`
mehrere `leased` bei Rückstau; Befund 5 → an der nächsten Recherche; Befund 1 → per Probe.

## Ergebnis NACH dem Upgrade (05.09.2026, Release branch-main-20260905T072559Z, umgeschaltet 07:53 UTC)

Upgrade 07:26:06–07:53 UTC (27,5 min), Start automatisch nach Ende der laufenden
BOOMEX-Recherche (die mit 23 Feldern abschloss, vorher 15). Beim Start war nur eine
Scraper-Reparatur geleast, keine Recherche.

| Befund | Messung nach dem Upgrade | Stand |
|---|---|---|
| 3 Kapazität | `ctox queue capacity` → `max_workers 4, workers_per_thread 1, scope independent business_os.chat.task sessions, storage SQLite runtime store` (explizit gesetzt). Eine Minute nach dem Umschalten **2 `leased`** gleichzeitig (07:53:19 / 07:53:20, beide Worker `21b39433…`, 15-min-Ablauf), 11 `pending`. Vorher nie mehr als 1. | **belegt** (2 von max. 4; mehr braucht mehr unabhängige Aufgaben im Rückstau) |
| 4 Projektion (neu) | Aufgabe `ebe4782bea95…` (Auth-Assist xing): Projektion `queued/pending/rev 1-civfwsvgrf` → `ctox queue cancel` im frischen CLI-Prozess → Projektion **`cancelled/cancelled`, lease_owner null, rev `2-1887bf89…`** um 07:56:57; Routing-State `cancelled`. | **belegt** |
| 4 Projektion (Altlast) | 7 Phantome unverändert (Reconciler `9b3f44e09` + Merge-Fix `5a16d0061` sind NICHT in diesem Release; sie belegen keine Kapazität). | offen bis zum zweiten Upgrade |
| 2 Reparatur-Dubletten | 4× `evi-gv-at` offen, alle **ohne** `scrape_repair`-Metadatum → vom alten Release erzeugt (nach meiner Bereinigung um 06:38, vor dem Umschalten). Drei per CLI storniert. Neue Einreihungen tragen das Metadatum; Dubletten-Freiheit über 24 h zu beobachten. | nicht widerlegt, 24-h-Messung offen |
| 1 Lease-Sweep | Zwei live Leases mit `lease_expires_at` (+15 min) und `lease_worker_id`. Probe „Worker stirbt" noch nicht gefahren. | Probe offen |
| 5 Writeback | 19/19 Leads mit Ergebnis, keiner mit 0 Feldern. App 1.0.100 liefert `payload.writeback_contract` mit `mechanism`/`command_type`. End-to-end (persistierter Vertrag → `business_os.execute_writeback` → Felder) braucht eine neue Recherche aus dem Browser. | **blockiert: Browser-Sitzung nach dem Upgrade abgemeldet**, Anmeldung durch den Eigentümer nötig |
| Regression Browser | — | **blockiert** (Anmeldung) |

Nächste Schritte: (a) Eigentümer meldet sich im Browser an → Regressionsliste + Befund-5-Lauf
(DrinkStar, 7 Felder); (b) zweites Upgrade mit `5a16d0061` (Reconciler + Merge-Fix) → Phantome 7 → 0;
(c) 24-h-Beobachtung Dubletten; (d) Probe Befund 1 bei Gelegenheit.

## Nachmessung 06.09.2026, 17:00 UTC (33 h nach dem Umschalten)

| Messung | Wert | Bewertung |
|---|---|---|
| Queue | 0 `leased`, 0 `pending` | leer, kein Rückstau |
| Befund 2 Dubletten | **0 doppelte offene Titel über 33 h** (vorher vier Wellen in 24 h) | belegt über den Beobachtungszeitraum |
| Befund 3 Kapazität | `max_workers 4`, in der Umschaltminute 2 parallel geleast | belegt (Maximum 4 nur mit ≥4 unabhängigen Aufgaben messbar) |
| Befund 4 neu | Cancel projiziert sofort (05.09. 07:56:57) | belegt |
| Befund 4 Altlast | 7 Phantome unverändert | offen — Reconciler `9b3f44e09`/`5a16d0061` nicht ausgeliefert |
| Befund 5 | Leads 19/19, 249 Felder gesamt, Minimum 7, keiner mit 0 Feldern. **Der neue Writeback-Guard hat noch nie gefeuert** (0 Treffer `Business command writeback failed`) — kein Task mit dem 1.0.100-Vertrag ist bisher gelaufen; der jüngste Nachrecherche-Befehl (05.09. 08:24) trägt noch den alten Vertrag (Daemon-Recovery kopiert das ursprüngliche Payload). | End-to-end offen |
| Seit Umschaltung `failed` | 14 Routing-Übergänge auf `failed`, darunter Beiersdorf („research contract is materially unmet: 2 von 8 Personen-Kategorien … Finite review budget exhausted 5/5") und eine Recovery-Aufgabe („10-path requirement structurally unsatisfiable"). **Das ist der Review-Mechanismus, der Endlosschleifen terminal stoppt — nicht der neue Guard.** Die Writebacks landeten trotzdem: Beiersdorf 12 → 18 Felder, BOOMEX 23. | erwartetes Verhalten, aber im CTOX-Modul als Fehler sichtbar |
| Testdaten | Writeback-Versuche mit Ids `…-writeback-test`, `…-test-v6-…` für BOOMEX vom Worker selbst; ein Versuch gegen `lead_test_001` wurde **abgelehnt**, kein solcher Lead existiert (54 Datensätze, 19 aktiv) | keine Verunreinigung |

**Blockiert:** Browser-Regression und Befund-5-End-to-end — die Browser-Sitzung ist seit dem
Upgrade abgemeldet, kein Chrome verbunden; Anmeldung nur durch den Eigentümer.

**Upgrade 2 zurückgestellt:** `origin/main` liegt 93 Commits / 38.865 Zeilen vor dem
ausgelieferten Stand (Sync-Engine 51 Dateien, RxDB 34, Shell 91; darunter „production peer
crash and data-preserving recovery", „rejected cutover and verified production rollback").
Ein `ctox upgrade --dev` würde das komplett und ungeprüft auf die Kundeninstanz bringen — nur
für sieben kosmetische Phantome. Entscheidung des Eigentümers.
