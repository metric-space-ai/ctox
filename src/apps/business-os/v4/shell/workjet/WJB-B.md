# WJB-B – Supervisor bind/turn/route und Planschritt wiederholen (Bericht Strang B)

Stand 16.10.2026, 05:30 Uhr. Klon `~/Documents/ctox-shell-v4` (Branch shell-v4/prototype, ee0ac39fb plus Arbeitsbaum).
Kein git benutzt, nichts committet. V2-Quellen: app.js = origin/main `e35a116ae` (`/Volumes/tmp/app-main.js`),
`ctox.plan.step.retry` = V2-PR #874 (`/Volumes/tmp/ctox-m20`, Branch shell/plan-step-retry, `f0c836569`),
Workjet = main `eb65947d` (`/Volumes/tmp/wj-measure`).

## Aktionen (11 von 11 freigeschaltet)

| Aktion | Stand | Orakel |
|---|---|---|
| project.supervisor.bind | freigeschaltet | project-control: „supervisor registration …“ (2); v4-Stub-Probe bind |
| project.supervisor.turn.submit | freigeschaltet | project-control: „explicit supervisor kinds …“, „supervisor submit watch cancel …“, „supervisor controls refuse forged …“; Stub-Probe turn |
| project.supervisor.turn.input | freigeschaltet | project-control: „Owner follow-up …“ (2); Stub-Probe turn |
| project.supervisor.turn.cancel | freigeschaltet | project-control: „submit watch cancel …“, „cancellation never pretends …“; Stub-Probe turn |
| project.supervisor.turn.watch | freigeschaltet | project-control: „turn results reject …“, „legacy supervisor watch …“, „opted-in supervisor watch …“, „public assistant text …“, „native Messages …“, „queued supervisor turn …“, „execution page …“ (3); Stub-Probe turn |
| project.supervisor.turn.capabilities | freigeschaltet | project-control: „supervisor capabilities …“ (2), „same-task input capability …“; Stub-Probe turn |
| project.supervisor.route.read.v1 | freigeschaltet | workjet-supervisor-route-native.test.mjs (22 Proben), route-display-contract (2); Stub nur über todo (K-WJ-6, s. u.) |
| project.supervisor.route.read.v2 | freigeschaltet | route-native, route-computation-contract (1); Stub-Probe route |
| project.supervisor.route.capabilities.v1 | freigeschaltet | route-native, route-display-contract; Stub-Probe route |
| project.supervisor.route.capabilities.v2 | freigeschaltet | route-native, route-computation-contract; Stub-Probe route + Zulassungszaun |
| ctox.plan.step.retry | freigeschaltet | workjet-plan-step-retry.test.mjs (3, aus PR #874); Stub-Probe retry |

Keine Aktion ist unsupported. Fehlt dem Kern eine Route, meldet sie „nicht unterstützt“ in der Form, die Workjet
erkennt (unknown_command → `Unsupported Workjet project control action: <befehl>`). Das belegt die Stub-Probe für
bind, turn.watch, route.read.v2 und plan.step.retry. Einschränkung für plan.step.retry: siehe K-WJ-B2.

Korpusproben ohne Brückencode (laufen gegen die generierten Module, die v4 importiert):
workjet-supervisor-source-contract (2), workjet-supervisor-luma-contract (1), workjet-worker-outcome-contract (2).
Die Validatoren für Ausführungsseite und Route-DTO benutzt die Brücke selbst.

Nicht übernommen, jeweils mit Grund:
- `project.supervisor.turn.history` mit den drei „Supervisor history …“-Proben: steht nicht in INVENTORY, Workjet ruft
  es nicht auf.
- route-native „actual Shell control uses Supervisor admission …“: Die Probe ersetzt die app.js-Globale
  `requireWorkjetSupervisorDataPlane`. Diese Globale gibt es in v4 nicht. Dieselbe Wirkung belegt die v4-Probe
  „Route-Zulassung: Instanzwechsel während business_commands-Bereitschaft verhindert den Versand“.
- luma-contract „project migration preserves …“: prüft modules/ctox/schema.js, gehört zu Strang A bzw. zum Kern.

## Orakel vorher/nachher

- Vorher (ee0ac39fb): `node --test 'v4/shell/workjet/tests/*.test.mjs'` mit 51 Tests, davon 42 V2-Orakel.
- Strang B ergänzt 53 V2-Orakel: 20 aus project-control, 22 route-native, 2 route-display, 1 route-computation,
  2 source, 1 luma, 2 worker-outcome, 3 plan-step-retry. Dazu kommen 7 Vollständigkeitsproben, eine je Orakeldatei.
- Außerdem 9 v4-Proben gegen den echten Kern-Stub (8489b4e): 7 grün, 2 als todo mit Kern-Lücke.
- Gesamtlauf nachher, mit den Blöcken aller Stränge: 248 Tests, 245 grün, 0 rot, 3 todo (2 von Strang B).
- Gegenprobe: Ich habe drei Stellen in supervisor.js absichtlich verfälscht (Route-Fehlertext, Retry-Status
  „queued“, turnKind fest auf „work“). Drei V2-Orakel wurden rot. Nach dem Zurücksetzen war alles wieder grün.

## Abnahme (16.10.2026, 05:27 Uhr, aus `src/apps/business-os/`)

```
node --test 'v4/shell/workjet/tests/*.test.mjs'  → tests 248 · pass 245 · fail 0 · todo 3 (Exit 0)
node v4/os-primitives/check-caps.mjs --calls     → calls: 202 · ABWEICHUNG 0 · HUELLE 0 · UNGEPRUEFT 3 · OK 195 · OHNE_VERTRAG 4
node v4/debt/ratchet.mjs                         → ratchet: FEHLER (guard.total 1838 → 1843)
node --test v4/tools/guards/guard-v4.test.mjs    → tests 74 · pass 74 · fail 0
```

Der Ratchet ist rot, aber nicht durch Strang B. `actions/supervisor.js` hat 0 Wächter-Treffer. Die Zuordnung der
+5 per `guard-v4.mjs --json`: `actions/meeting.js` 4 (vocab:en-unavailable 3, en-pending-waiting 1) und
`actions/project.js` 1 (vocab:en-unavailable). Die Testdateien zählt der Ratchet nicht.

## Geänderte Dateien

- neu: `v4/shell/workjet/actions/supervisor.js` mit `SUPERVISOR_ACTIONS` (11 Aktionen) und dem Export
  `requestSupervisorRoute`, wörtlich aus shared/workjet-supervisor-route-native.mjs (das Orakel lenkt den Import
  auf diese Datei um).
- neu: `v4/shell/workjet/WJB-B.md` (dieser Bericht)
- additiv `v4/shell/workjet/index.js`: ein Import und eine Zeile `...SUPERVISOR_ACTIONS` in `ACTIONS.project`
  (`// Strang B`)
- additiv `v4/shell/workjet/tests/oracle.test.mjs`, drei Blöcke `// Strang B`:
  - die Testnamen aus project-control,
  - eine eigene Orakel-Hülle `materializeB`. Sie materialisiert die V2-Testdatei samt
    `src/core/rxdb/tests/fixtures/*.json` aus Pin oder PR, lenkt route-native.mjs auf supervisor.js um und nimmt die
    generierten Module aus dem v4-Arbeitsbaum,
  - die v4-Stub-Proben.
- additiv `v4/shell/workjet/tests/contract-checkers.mjs`: 11 Prüfer nach Workjet ctox.ts:873-877 und :1113-1185,
  workjetSupervisor.ts und workjetSupervisorRoute.ts. Die Route-DTOs und die Ausführungsseite prüfen sie über die
  generierten Validatoren. Zusatzschlüssel gelten als Fehler, weil Workjet mit `onExcessProperty: 'error'` dekodiert
  (CtoxGuestManager.ts:2474).

## turn.watch und Zwischenstände (Auftrag: genau dokumentieren)

- So arbeitet V2: `turn.watch` ist kein Strom, sondern ein eigener Befehl
  (`ctox.workjet.project.supervisor.turn.watch`, `until: 'terminal'`). Er liefert den Momentzustand des Ziel-Turns:
  `execution_phase`, `status`, `queue_status`, `attempt`, `terminal`, `result` und optional eine Ereignisseite mit
  Cursor. Zwischenstände entstehen, weil Workjet wiederholt abfragt. Die Pulsfrequenz liegt bei 3 s, solange ein
  Turn offen ist (NativeSupervisorComposer.tsx:662). Weitere Ereignisseiten holt Workjet im Abstand von 100 ms nach
  (:672).
- Das liefert v2-compat heute:
  - `dispatch(until 'terminal')` wartet über das Status-LiveDoc auf done, failed oder cancelled. Die Antwort trägt
    `result` aus dem Status. Für watch reicht das: Jeder Puls ist ein abgeschlossener Befehl.
  - `dispatch(until 'accepted')` endet bei accepted oder running. Die Quittung hat dann aber `ok: false` (siehe
    unten, gehört nach v2-compat).
  - Ein Kern-`running` mit `result` geht nur bei `requestNative` an `opts.onPartial`. `commandBus.dispatch` hat
    keinen Zwischenstandskanal. Strang B braucht keinen.
- Was fehlt:
  1. Jeder watch-Puls erzeugt ein neues `business_commands`-Dokument (K-WJ-4, keine vom Aufrufer vergebene ID).
     Bei 3 s je offenem Turn sind das rund 1 200 Dokumente pro Stunde und Turn. Dazu kommen die Ereignisseiten im
     Abstand von 100 ms.
  2. Null-Felder im Momentzustand gehen verloren (K-WJ-6). Danach lehnt Workjet die Antwort ab.

## Gehört nach v2-helpers

- `requireWorkjetSupervisorDataPlane(env)` (ref app.js:15002-15008): V2 nutzt diese Zulassung auch für kpis.*,
  exit_model.*, project.init und jour_fixe.meeting.* (app.js:14049-14061), also auch für Strang A und C.
  Wörtliche Fehlermeldung „Workjet supervisor control is not ready.“, die Workjet vergleicht
  (CtoxGuestManager.ts:527, :985).
- `boundedWorkjetExecutionRequest` (ref app.js:13838-13849). Auch `turn.history` bräuchte
  `boundedWorkjetHistoryRequest`.
- `scope(env)`, der Vorspann aus app.js:13906-13917. Er liegt jetzt doppelt vor, in project.js und supervisor.js.
- `requestSupervisorRoute`, der Port von shared/workjet-supervisor-route-native.mjs. Wenn der Orchestrator ihn
  verlegt, muss die Umlenkung in `materializeB` mitziehen.

## Gehört nach v2-compat (Befund mit Wirkung)

Mit `until: 'accepted'` liefert v2-compat bei den Kernständen `accepted` und `running` die Quittung mit `ok: false`,
denn `ok` steht dort nur bei `status === 'done'` auf true. V2 (shared/command-bus.js:1653-1664 `commandReceipt`)
liefert in diesem Fall `ok: true` und wirft bei failed. Der wörtliche V2-Rumpf von plan.step.retry hätte deshalb jeden
angenommenen Retry als „Plan step retry was rejected.“ gemeldet, also ein falsches „fehlgeschlagen“. Abhilfe bis zur
Korrektur in v2-compat: die lokale Funktion `acceptedReceipt` in supervisor.js. Sie setzt nur dann `ok: true`, wenn
`status` accepted oder running ist und kein `error` vorliegt. Die Stub-Probe „ctox.plan.step.retry wartet nur auf
accepted …“ belegt das. Vorschlag für v2-compat: Bei `until: 'accepted'` soll `ok = status ∉ {failed, cancelled}`
gelten. Danach kann `acceptedReceipt` entfallen.

## Offene Fragen an den Kern

- **K-WJ-6** (auch von Strang A gemeldet, intern B1): Der Stub schreibt `business_commands.result` per JSON-Merge-Patch
  (`mergePatch`: null löscht das Feld). Gilt das auch im echten Kern, dann verlieren alle Supervisor-Antworten ihre
  Null-Felder: turn `error_code`/`error_message`, `task_id`/`queue_status`, Route v1 `actual: null`. Workjet verlangt
  diese Felder als Pflichtfelder (`NullOr`), die Antwort scheitert also mit guest_failed. Route v1 read scheitert
  schon in V2 an „unproved producer“, weil `actual` dann undefined statt null ist. Belegt als todo-Probe
  „null-Felder im Befehlsergebnis erreichen Workjet“. Lösung im Kern: `result` als ganzen Wert ersetzen statt
  per Merge-Patch einmischen.
- **K-WJ-B2**: Eine unbekannte Befehlsart muss der Kern vor `accepted` ablehnen, als write_result
  `rejected{unknown_command}`. Meldet er sie erst nach accepted als failed (so simuliert es der Stub mit
  `failCommand`: accepted → running → failed), dann antwortet `ctox.plan.step.retry` mit `pending`. Workjet zählt das
  als „wiederholt“, statt den Knopf als unsupported auszublenden. Belegt als todo-Probe.
- Route-DTO-Dekoration: V2 erwartet im Ergebnis `status: 'completed'` und `task_status: 'completed'`, die der
  command_plane hinzufügt (route-native.mjs:47-53). Liefert der v4-Kern das DTO ohne diese Dekoration, endet route.*
  in „Supervisor route returned incomplete native result status.“. Das wäre ein „fehlgeschlagen“, kein
  unsupported. Der Kern muss die Dekoration liefern, sonst muss v2-compat sie für `ctox.workjet.project.supervisor.route.*`
  ergänzen.
- `record_id` von plan.step.retry ist die `step_id` (V2). Unklar ist, ob der v4-Kern record_id als
  Berechtigungs- oder Projektionsschlüssel gegen ein Projekt prüft.

## Betriebshinweis

Bei einem hängenden Lauf habe ich einmal `pkill -f oracle.test.mjs` ausgeführt. Das kann parallel laufende
Testläufe anderer Stränge beendet haben; dauerhaft beschädigt ist dadurch nichts. Danach habe ich nur noch Läufe mit
eigener Zeitgrenze gestartet (perl alarm). Den Rahmen-Labor-Test habe ich nicht gestartet, wie vom Brief vorgegeben.
