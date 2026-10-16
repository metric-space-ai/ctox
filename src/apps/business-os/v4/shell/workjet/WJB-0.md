# WJB-0 – Gerüst der Workjet-Brücke in Shell v4 (Bericht)

Stand 16.10.2026, Klon `~/Documents/ctox-shell-v4` (Branch shell-v4/prototype, HEAD 41c4eef3b + Arbeitsbaum).
Quellen: V2 = origin/main `e35a116ae` (`/Volumes/tmp/app-main.js`, Orakel per `git show e35a116ae:…`), Workjet main
`eb65947d` (`/Volumes/tmp/wj-measure`), Kern-Stub-Pin `8489b4e`. Kein git außer Lesen, nichts committet.

## Dateien

| Datei | Inhalt |
|---|---|
| `v4/shell/workjet/v2-compat.js` | Adapter: V2-`state` (commandBus, session/actorContext, sync.requestNative, sync.startCollection, db, syncConfig) über der Shell-App-API; typisierter Fehler `{code:'unsupported'}` |
| `v4/shell/workjet/v2-helpers.js` | gemeinsame V2-Hilfen wörtlich (Textgrenzen, Listen-Schritte, Projekt-/Kopie-Ergebnis, Metadaten/Feed) |
| `v4/shell/workjet/index.js` | Router je Brücke, `INVENTORY` (76 Aktionen, Strang-Zuordnung), `installWorkjetBridges` |
| `v4/shell/workjet/actions/project.js` | Strang A; portiert: `project.list`, `project.worker.add` (`project.chat.create` vorbereitet, nicht freigeschaltet) |
| `v4/shell/workjet/tests/v2-oracle.mjs` + `oracle-*-shim.mjs` | Orakel-Harness: lädt die V2-Testdatei unverändert aus `e35a116ae`, tauscht nur app.js gegen eine Hülle zur v4-Brücke |
| `v4/shell/workjet/tests/oracle.test.mjs` | freigegebene V2-Tests je Datei (`ORACLE_TESTS`) |
| `v4/shell/workjet/tests/contract-checkers.mjs` | JS-Prüfer gleichwertig zu den effect/Schema-Pflichtfeldern aus `packages/contracts/src/ctox.ts` |
| `v4/shell/workjet/tests/bridge.test.mjs` | Ende-zu-Ende gegen den echten Kern-Stub (memory): Adapter, Router, Vertrag, E25 |
| `v4/shell/workjet/PORT-PLAN.md` | Stränge A–D, disjunkte Dateien, Kern-Abhängigkeiten |
| `v4/frame/index.js` | EIN Edit in `startShell`: `import('../shell/workjet/index.js').then(m => m.installWorkjetBridges({kernel, shellVer}))` |

## Abnahme (selbst gelaufen)

```
node --test v4/shell/workjet/tests/oracle.test.mjs   → 43/43 (42 V2-Tests gegen v4 + Vollständigkeit; 69 V2-Tests der Datei nicht portiert)
node --test v4/shell/workjet/tests/bridge.test.mjs   → 8/8, Prozess endet sauber
node v4/frame/lab/frame.test.mjs mock                → vorher 167/167, nachher 167/167, Exit 0
CTOX_APP_API_STUB=…/8489b4e node v4/frame/lab/frame.test.mjs stub → vorher 208/208, nachher 219/219, Exit 0
  (erster Nachher-Lauf 161/165: Browser mitten im Lauf geschlossen, „Target page … has been closed“, Last 4,8;
   Wiederholung allein 219/219. Die höhere Zahl gegenüber vorher stammt aus dem Labor selbst, nicht aus WJB-0.)
node v4/os-primitives/check-caps.mjs --calls         → calls 202 · ABWEICHUNG 0 · HUELLE 0 (unverändert)
node v4/os-primitives/check-caps.mjs                 → OK, 4 app/platform caps within ratchet baseline (unverändert)
```
Gesamt: `node --test 'v4/shell/workjet/tests/*.test.mjs'` → 51/51 (Node 26 braucht das Glob, ein Verzeichnis geht nicht).
(Lauf aus `src/apps/business-os/`.) Hinweis: während der Läufe änderte eine Parallelsitzung (ZONE2) `v4/frame/index.js`,
`frame.css` und `lab/frame.test.mjs`; vom WJB-0-Edit stammen nur die zwei Zeilen nach `const shellVer` in `startShell`.

Orakel im Einzelnen: `project.list` 26 V2-Tests (Legacy-Form, Alias-Owner, Reparaturlesen, Generationswechsel,
Seitenfenster 200/500, Fristen, Zähler-/Vollständigkeitsprüfung, Opt-ins Konfiguration/Luma/Policy),
`project.worker.add` 16 V2-Tests (Weiterleitung, 11 gefälschte Quittungen, Sitzungs-/DB-Wechsel, Policy-Ablehnung,
kein Versand nach Sitzungswechsel). Nicht übernommen: „Workjet validates chat request fields before dispatch“ (mischt
`project.chat.create`), Quelltext-Regex-Tests auf app.js (keine Verhaltensorakel).

## Adapter-Abbildung (v2-compat.js)

- `state.commandBus.dispatch(cmd, {until, timeoutMs})` → `api.command(cmd.command_type, cmd.payload, {record_id})`,
  Warten über das gepushte `status`-LiveDoc bis `accepted` bzw. `done|failed|cancelled`, Frist → `REQUEST_TIMEOUT`.
  Quittung in V2-Form `{command_id, ok, status: completed|failed|cancelled, target_record_id, payload, result, error}`.
  `command_id` = ID des V2-Aufrufers (der v4-Kernel vergibt die Dokument-ID selbst → `kernel_command_id`);
  `payload`/`record_id` kommen aus dem gespeicherten `business_commands`-Dokument (`api.get`), sonst aus dem Befehl.
  `failed` mit `unknown_command…` und write_result `rejected` mit unknown_command → `{code:'unsupported'}`.
- `state.session`/`actorContext` → ein Sitzungsobjekt je Brücke; Akteur aus `api.actor` + `kernel.identity`.
  `dispose()` (bei Shell-Tausch) setzt Sitzung/DB auf null → alle V2-Zäune „session changed“ greifen.
- `state.sync.startCollection(name)` → V2-Brückenform `{state: peer}`: `awaitQueryReady` = Collection-Liveness `live`
  (§16.45; ohne Eintrag gilt lokal bereit, `error` → `PEER_UNAVAILABLE`), Generation = `collections[name].gen`
  (fehlender Eintrag = g1), `find(q).exec()` = `api.query(name, {selector, sort, limit, skip}).get()`.
- `state.db.collection(name)` → `find/findOne/$.subscribe` über `api.query`/`api.get`.
- `state.sync.requestNative(method, params, opts)` → **E25**: `api.command(method, params)`, Antwort =
  Befehlsvollendung; `running` mit `result` → `opts.onPartial(result)`; `unknown_command` → unsupported;
  andere Fehler → `Error{code}` aus dem Präfix `<code>:`. `opts.requiredCapability` wird nicht weitergereicht (prüft
  der Kern). Hinweis: E25 nennt die Form `kernel.command({type, payload})`; der gepinnte Vertrag (index.d.ts 8489b4e)
  ist `api.command(type, payload, opts)` – der Adapter nutzt den gepinnten Vertrag.
- `state.syncConfig.instance_id` → `kernel.identity.instance_id`.
- Uhr: `env.now()` (V2-Tests setzen `Date` im vm-Kontext; ohne diese Abhängigkeit scheiterten 2 Frist-Orakel).

## E25: requestNative-Methoden der V2-Brückenfunktionen (Abnahmeliste für den Kern)

Erhoben über alle V2-Dateien mit `requestNative` (Pin e35a116ae; app.js, shared/*.mjs), eingeschränkt auf die fünf
Workjet-Brücken. Genau 6 Methoden:

| Methode (= Kern-Befehlstyp) | aufrufende Workjet-Aktion(en) | V2-Fundstelle | requiredCapability / Frist |
|---|---|---|---|
| `ctox.workjet.speech.dictation.v1` | `speech.dictation` (op open/write/read/finish/cancel) | shared/speech-dictation.mjs:3,60 | ctox-workjet-speech-dictation-v1; 20 s open, sonst 5 s |
| `ctox.workjet.speech.settings.v1` | `speech.settings.read`, `.configure`, `.key`, `.playback`, `.voices`, `.check`, `.check.transcription` | shared/speech-settings-control.mjs:13 | ctox-workjet-speech-settings-v1; 25 s, check.transcription 30 s |
| `ctox.workjet.calendar.read.v1` | `project.calendar.accounts.read`, `project.calendar.events.read` | shared/workjet-calendar-native.mjs:5,30 | – |
| `ctox.workjet.jour_fixe.speech.v1` | `project.jour_fixe.speech` | shared/jour-fixe-speech.mjs:14, jour-fixe-speech-contract.mjs:1 | ctox-workjet-jour-fixe-speech-v1; 20 s open, sonst 5 s |
| `ctox.workjet.grok.v1` | `instance.grok.read`, `.start`, `.poll`, `.cancel`, `.check`, `.remove` | shared/workjet-grok-native.mjs:3,16 | ctox-workjet-grok-v1; 24 s |
| `ctox.workjet.device.v1` | `binding.list`, `binding.revoke`, `invite.create`, `invite.revoke` (workjetBusinessOsDeviceControl) | app.js:7874 | ctox-workjet-device-control-v1; 10 s |

Nicht über requestNative (also schon Kern-Befehle über commandBus): Provider (`instance.providers.*`,
workjet-provider-native.mjs:151), Supervisor-Route (workjet-supervisor-route-native.mjs:40), alle `project.*`
außer den obigen, Rechner, Sitzung. Narration/Präsentation lesen Dateibereiche über `desktop_files` +
`demandFileLoader.fetchFile(fileId,{range})` (app.js:13987-14000, 15556-15569) – kein requestNative, aber eine eigene
Kern-Lücke (K-WJ-2).

## Fehlerform „nicht unterstützt“ (Befund aus Workjet, wichtig für alle Stränge)

Workjet liest `code` NICHT einheitlich (`apps/desktop/src/ctox/CtoxGuestManager.ts`, eb65947d):
- Projekt: nur der **Nachrichtentext** `/(?:unsupported|unknown)\s+(?:workjet\s+)?(?:project[- ]control\s+)?action/i`
  → `unsupported`. Der Fehler trägt daher `code:'unsupported'` UND die V2-Nachricht
  `Unsupported Workjet project control action: <a>`.
- Rechner: nur die Nachricht `workjet_computers collection is not registered.` → unsupported (sonst command_failed).
- Gerät: `code` `CTOX_WEBRTC_CAPABILITY_MISSING`/`ctox_webrtc_unavailable` → unsupported (Fehler trägt `nativeCode`;
  Strang D muss beim Einhängen `code` auf dieses Merkmal setzen, sobald die Gerätebrücke kommt).
- Sitzung und Sitzungsereignisse: kein try/catch, nur „Global fehlt“ → unsupported bzw. `registered: 0`.
Deshalb hängt `installWorkjetBridges` eine Brücke nur ein, wenn sie ≥ 1 freigeschaltete Aktion hat (heute nur
`workjetProjectControl`). Abweichung vom Brief („5 Globals registrieren“) mit Absicht: ein eingehängtes, leeres
Global würde Workjet bei Sitzung/Rechner „fehlgeschlagen“ statt „nicht unterstützt“ zeigen.
`workjetHostBridge`/`workjetBusinessOsPostMessage` stellt der Host (Workjet-Preload, Mobil-App) bereit; die Shell
konsumiert sie nur über mobile-host.js (Strang D, V2-DOM-Abhängigkeiten `.shell-window`, `is-focused`).

## Kern-Lücken (für den Kern)

- **K-WJ-1 exports**: das ctox-Manifest exportiert `workjet_projects` (read), nicht `workjet_working_copies`,
  `workjet_session_transfers`, `workjet_computers`, `workjet_sessions`, `workjet_jour_fixe_meetings`. project.list
  liest Arbeitskopien; der Stub prüft exports nicht, im echten Kern (§16.13) wäre das Lesen verweigert.
- **K-WJ-2 Bereichslesen**: `files.get` ohne Range; Narration/Präsentation brauchen Bytebereiche.
- **K-WJ-3 Diktat**: V2 = ein Request je Operation; E25 = Zwischenstände unter einer command_id. Zuordnung offen
  (PORT-PLAN, Strang C).
- **K-WJ-4 Client-Befehls-ID**: `api.command` nimmt keine vom Aufrufer vergebene ID; V2-Idempotenz über commandId
  (Workjet wiederholt mit derselben ID) geht in v4 verloren – jede Wiederholung erzeugt ein neues Befehlsdokument.
  Vorschlag: `opts.client_id`/`idempotency_key` im Kern.
- **K-WJ-5 Befehlslaufzeiten**: V2 liefert `diagnostics.onCommandTiming` (Workjet-Diagnose); v4 hat keine Marken.
  Die Diagnose bleibt ohne Wirkung (Orakel `workjet-project-command-timing.test.mjs` nicht übernommen).

## Weitere Befunde

- **guard-v4 (Löschliste, Zählmodus)**: die neuen Dateien tragen absichtlich V2-Vokabular (`commandBus`,
  `startCollection`, `requestNative`, `until:'terminal'`, `requireRevision`): 25 Treffer in `v4/shell/workjet/`,
  Exit 0. `v4/shell/workjet/` darf erst dann in `guards/ported.json`, wenn der Kern die Brücke als Kompatibilitätszone
  ausnimmt oder die Namen nach dem Cutover umbenannt werden.
- **Shell-Manifest**: `v4/manifests/shell.manifest.json` listet die neuen Dateien noch nicht (generiert, nicht im
  Whitelist dieses Strangs). Im Labor (Dateiserver) unkritisch; im SW-Artefakt lädt der dynamische Import erst nach
  Neuerzeugen des Manifests (gen-manifest). Bis dahin fängt `startShell` den Importfehler ab (Warnung, kein Absturz).
- Der dynamische Import im Rahmen ist bewusst asynchron: die Globals stehen kurz nach `startShell`, nicht synchron.
  Workjet ruft sie erst nach Bereitschaft auf; ein früher Aufruf sieht „Global fehlt“ = unsupported (gutartig).
- Bei Shell-Tausch ersetzt der neue Start die Globals und beendet die alte Sitzung (laufende Aufrufe der alten Shell
  scheitern an „session changed“ wie in V2 beim Sitzungswechsel).
