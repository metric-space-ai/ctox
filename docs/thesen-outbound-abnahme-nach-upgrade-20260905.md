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
2. Nach dem Umschalten: `ctox queue capacity --workers 4`.
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
