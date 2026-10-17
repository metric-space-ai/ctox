# CAP3B – App-Handler gegen die Kernel-Capabilities CAP-3b

Stand 15.10.2026, Strang CAP3B (Shell v4). Vertragsquelle: PR metric-space-ai/ctox#844 (Zweig `v4/cap-3b` @ b877621a6,
`caps/cap3.rs` ROUTES + `contract()`, `files.rs`, `process.rs`, `directory.rs`, `browser.rs`), Architektur §16.29 ff., §16.46.
Stub für die Tests: `~/.local/state/shell-v4/stub-pins/74137b1`.

## Zahlen (check-caps.mjs --calls)

| | Aufrufe | ABWEICHUNG | HUELLE | UNGEPRUEFT | OK | OHNE_VERTRAG |
|---|---|---|---|---|---|---|
| vorher | 204 | 0 | 0 | 2 | 180 | 22 |
| nachher | 199 | **0** | 0 | 2 | 190 | **7** |

Fünf Aufrufe weniger: `files.materialize`, `model.run` (jetzt `ctx.model`) und 2× `agent.command.settle` sind weg, dazu ein
Doppelaufruf `secret.scope`. Die übrigen 7 OHNE_VERTRAG: `pentest.*` ×5 (App-Logik, Port offen, siehe unten),
`browser.automation.run` und `artifact.write` (outbound.source.import; weder in CAP-3 noch in CAP-3b gebaut).

Manifest-Wächter (`check-caps.mjs`): OK, 66 OS-Primitive, 9 in der Ratchet-Baseline (vorher 73 / 2).

## Vertrag nachgezogen (core-contract.mjs)

Neue flache Routen nach #844: `files.put|get|list|delete`, `process.run`, `browser.auth_assist.request`,
`directory.user`, `identity.users.list`, `policy.module_decision`, `command.get`. Neue vollständige Familien:
`files.` `process.` `directory.` `identity.users.` `policy.` `command.` (Fehlgriffe zählen damit als ABWEICHUNG).
`files.materialize` als Umbenennung auf `files.get`. `OPEN_IN_CORE` trägt für `model.run`, `agent.command.settle` und
`pentest.*` jetzt die Entscheidung aus #844 („kein OS-Primitiv“) statt „offen“.

## Katalog (catalog.mjs)

- `model` als Primitiv entfernt, `agent.command.settle` aus `agent.queue`, `pentest.` aus `process.run`.
- Alle sieben als App-Logik eingetragen (`model.run`/`agent.command.settle` mit dem Hinweis „aus capabilities_required
  entfernen“, `pentest.*` → appsec-pentest über process.run + files).
- `check-caps.baseline.json` neu geschrieben: alt `agent.flow.record`, `command.get`, neu `model.run`,
  `agent.command.settle` und `pentest.*` ×5. Damit bleibt der Wächter grün, bis MAN2 die Manifeste bereinigt.
- MAPPING.md (gen-mapping) und CALL-CONTRACT.md (`--calls-md`) neu erzeugt.

Einen eigenen check-caps-Unit-Test gibt es im Klon nicht. Abgedeckt sind `--calls`, der Wächter,
`gen-mapping --check` sowie guard-v4/isolation/gen-manifest (123/123 grün).

## Handler-Änderungen je App

| App | Stelle | Änderung |
|---|---|---|
| OLG | `_lib/scrape-registry.js` | `files.get({ref})` → liest `text`; `files.put({name, mime, text})` → `{id, ref}`; keine Fremdfelder (owner_app/collection). `not_found` → null; **`unavailable` → typisierter Fehler**. Das Absorbieren zu null hätte das Skript als fehlend gewertet und eine Neugenerierung ausgelöst. |
| OLG | `_lib/scrape-runner.js` | Kein `files.materialize` mehr. Das Skript (über files.get gelesen, sha256 geprüft) geht auf **stdin**, `{script_path}` → `-`. `program` = Allowlist-Name (Pfad wird auf den Basisnamen gekürzt). **Leere Umgebung + explizites env**: CTOX_SCRAPE_TARGET_KEY/TARGET_DIR/RUN_DIR/OUTPUT_DIR/RUN_ID/TRIGGER_KIND/START_URL/SOURCES_JSON/MANIFEST_JSON/COMMAND_ID/INPUT_JSON, CTOX_OWNER_USER_ID. cwd = `<workspace>/runs/<run_id>`. Werte über 4096 Byte → `invalid` vor dem Aufruf. Ergebnis `stdout`/`stderr` als Strings (vorher `*_ref`). |
| OLG | `_lib/research-source-adapter.js` | auth_assist: nur Vertragsfelder; `credential_ref` = `ctox-secret://credentials/<name>` (Kern akzeptiert nur das); owner_user_id/deterministic_for_task/accept_as_trusted_local entfallen. Generierungs-Prompt: Eingabe über env statt stdin-Umschlag. |
| OLG | `_lib/research-writeback.js` | `command.get` → `{found, …}`; `found:false` → „existiert nicht“. |
| OLG | `index.js` `registryKontext` | Browser-seitiger Leser derselben Bibliothek: Adapter `cap.files.get/put` nach CAP-3b über `api.files` (Stub-Abweichung, s. u.). |
| outbound | `_lib/olg-registry.js`, `_lib/research-adapter.js`, `_lib/scrape-generation.js` | files.get({ref}) wie OLG; auth_assist ohne Fremdfelder; Prompt wie OLG. |
| news | `news.source.auth_assist.js` | auth_assist ohne Fremdfelder; die Owner-Pflicht bleibt als App-Prüfung bestehen. |
| kundenpipeline | `_lib/hub.js` | `identity.users.list` → `result.users`. |
| threads | `_lib.js` | `directory.user` → `found !== true` ⇒ kein aktiver Nutzer. policy.module_decision war schon konform. |
| research | `_lib/records.js` | `agent.command.settle` entfernt (Befehls-Lebenszyklus). Status/Ergebnis trägt der Rückruf-Befehl. Der Fehlstand steht in research_runs/research_tasks. Tote Helfer entfernt. |
| iot | `_codegen.js` | `ctx.cap.model.run(…)` → `ctx.model(…)` (App-Laufzeit, #830). |
| appsec-pentest | – | **nicht portiert**, siehe offen. |

## Tests (CTOX_APP_API_STUB=…/74137b1, `node --import ./v4/tools/node/ctox-shared.mjs --test …`)

OLG 151/151 · appsec-pentest 33/33 · iot 64/64 · kundenpipeline 47/47 · news 51/51 · outbound 176/176 · research 100/100 ·
threads 95/95. Vorher: OLG 149, iot 64, Rest gleich, alle grün.

Der Stub-Pin kennt `ctx.cap` nicht. Deshalb sind alle Cap-Attrappen in den App-Tests **nach Vertrag** gebaut und markiert
(„Attrappe nach Vertrag“): files mit sha256-ref und deny_unknown_fields, process.run mit Allowlist-/env-Prüfung,
command.get/directory.user mit `found`, identity.users.list mit `{users}`, iot mit `ctx.model`, research ohne settle.
Neue Tests in OLG `scrape-registry.test.mjs`:
- files `unavailable` ⇒ Lauf scheitert mit Code `unavailable`, kein process.run, kein Laufdokument;
- env-Wert über 4096 Byte ⇒ `invalid`; Pfad-Programm ⇒ Allowlist-Name;
- Skript auf stdin, CTOX_OWNER_USER_ID gesetzt.

## Stub-Abweichung (vermerkt)

Stub-Pin 74137b1 (neuester Pin, `git log` im Stub-Repo) liefert `api.files` noch in der alten Form:
`get(ref:string) → Blob`, `put(blob, meta) → FileUploadHandle`. CAP-3b kennt er nicht: weder `files.get({ref}) → {text}`
noch `files.put({name, mime, text}) → {id, ref}`. Dazu kein `ctx.cap` und kein `ctx.model`. Der Live-Test
`outbound-lead-generation/tests/v4-api.test.mjs` („Scrape-Registry live“) läuft deshalb über den Adapter in
`index.js registryKontext`. Der Adapter entfällt, sobald Kern bzw. Stub die CAP-3b-files-Bindung ausliefern.

## Offen / an andere Stränge

- **MAN2 (Manifeste)**:
  - `iot.manifest.json`: `model.run` aus capabilities_required streichen. Für ctx.model ist der Grant `model` nötig (#830 `capability_available("model")`).
  - `research.manifest.json`: `agent.command.settle` streichen.
  - `appsec-pentest.manifest.json`: `pentest.*` ×5 streichen, sobald portiert; dann `process.run` (+ files) anfordern.
  - Die Baseline-Einträge danach wieder entfernen.
- **appsec-pentest-Port (eigene Scheibe, Aufwand L)**: `pentest.run`/`app_audit` → `process.run` mit Engine-Allowlist-Namen.
  - Die Rust-Seite (`run_projected_appsec_command`, main.rs:1603) hängt den Credential-Nachweis an, reiht Pipeline-Aufgaben ein und projiziert in den dauerhaften Zustand. Das ist App-Logik und wäre mitzuportieren.
  - `exploit_list/get/verify` lesen Index und Skripte aus dem State-Dir; das geht nur über files.get, also erst nach dem files-Cutover.
  - Nicht in diesem Strang, weil weder prüfbar (files `unavailable`) noch im Zeitbudget.
- **Kern**:
  - (a) Mit leerer Umgebung fehlen PATH und CTOX_BIN. Die Scrape-Skripte rufen `ctox web …` über `CTOX_BIN || "ctox"` auf, die App kennt den Hostpfad nicht. Nötig ist eine generische Lösung im Kern (z. B. ein Allowlist-Eintrag, den der Kern als CTOX_BIN einsetzt).
  - (b) `files.get` ist auf owner_app begrenzt; die outbound-Registry-Sicht liest Skripte, die OLG gehören. Ohne Fremdzugriff auf files liefert der Kern dort `not_found`.
  - (c) env-Werte sind auf 4096 Byte begrenzt. Große Ziel-Configs (CTOX_SCRAPE_MANIFEST_JSON) scheitern dann mit `invalid`.
  - (d) `tsx` als Programm liest das Skript nicht von stdin (`-`); betroffen sind nur TypeScript-Skripte.
- **OLG-Doku**: `customer-modules/outbound-lead-generation/V4-HANDLERS.md` Z. 63 und Z. 102 beschreiben noch `files.materialize` und den stdin-Umschlag. Nicht geändert, weil der Strang für OLG nur Handler-Code und Tests umfasst.
- **OLG secret.exists**: OLG prüft das Login-Secret im eigenen App-Scope. auth_assist verweist jetzt aber auf `credentials`. Liegt das Secret nur im App-Scope, lehnt der Kern den Verweis ab. Fachlich zu klären, wo OLG-Quellen-Logins liegen.
