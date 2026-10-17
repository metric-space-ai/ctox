# Workjet-Brücke v4: Plan für die Parallel-Stränge (WJB-0 → A–D)

Ziel: zum V4-Cutover auf welsch (Sa 14:00, M7) beantwortet die v4-Shell jede Aktion, die Workjet heute aufruft,
mit unveränderter Schnittstelle (E24). Abnahmeliste: `INVENTORY` in `index.js`, 76 Aktionen (gemessen in
`~/.local/state/shell-v4/analysis/workjet-bridge-actions.md`: Projekt 60, Rechner 6, Sitzung 6, Gerät 4).

## Spielregeln (für alle Stränge gleich)

1. **Eine Datei je Strang**: `actions/<gruppe>.js`. Ausnahme: zwei Einträge pro Strang in `index.js` (ein Import,
   eine Zeile im Objekt `ACTIONS`) und die Testnamen in `tests/oracle.test.mjs` (`ORACLE_TESTS`) sowie die Prüfer in
   `tests/contract-checkers.mjs` (`CHECKERS`). Diese drei Dateien nur additiv ändern (eigener Block je Strang), damit
   Merges konfliktfrei bleiben.
2. **Rumpf wörtlich V2** (`/Volumes/tmp/app-main.js` = origin/main app.js, Pin `e35a116ae`), mit `// ref:`-Anker.
   Nur `state.` → `env.state.`, `actorContext` → `env.actorContext`, `newId` → `env.newId`, `Date.now()` →
   `env.now()` (V2-Tests setzen die Uhr im vm-Kontext). Gemeinsame Hilfen kommen aus `v2-helpers.js`; fehlt eine,
   meldet der Strang sie an WJB-0 (Kern-Strang), statt sie in die eigene Datei zu kopieren.
3. **Freischalten = Orakel grün**: eine Aktion kommt erst in die exportierte Tabelle `<GRUPPE>_ACTIONS`, wenn ihre
   V2-Verhaltenstests in `ORACLE_TESTS` eingetragen und grün sind und ein Prüfer in `CHECKERS` steht.
   `bridge.test.mjs` schlägt fehl, wenn eine freigeschaltete Aktion keinen Prüfer hat.
4. **Fehlende Kernteile** nie umgehen: `throw unsupportedError(bridge, action, '<Lücke>')` (v2-compat.js). Workjet
   blendet den Knopf dann aus. Jede so gemeldete Lücke steht im Strangbericht mit Kern-Abhängigkeit.
5. Abnahme je Strang: `node --test 'v4/shell/workjet/tests/*.test.mjs'` grün, `node v4/frame/lab/frame.test.mjs mock|stub` grün,
   `node v4/os-primitives/check-caps.mjs --calls` ABWEICHUNG 0.

## Stränge

### A – Projekt, KPI, Exit, Präsentation → `actions/project.js` (besteht, WJB-0 hat project.list + project.worker.add)
Aktionen: project.create, project.init, project.configure, project.chat.create (Rumpf liegt als `PREPARED` bereit),
project.kpis.read|configure|refresh, project.exit_model.read|refresh|submit,
project.presentation.read|content.read|canvas.save.
Orakel: `workjet-project-control.test.mjs` (create/configure/KPI/policy/Luma-Blöcke), `workjet-exit-model-control.test.mjs`,
`workjet-projection-readiness.test.mjs`, `jour-fixe-presentation.test.mjs`.
Kernabhängigkeiten:
- project.create/configure warten in V2 auf die Projektion (`waitForProjectedWorkjetProject`, `waitForSyncBridgeReady`);
  in v4 über `startCollection`-Brücke + `findOne` (v2-compat) – funktioniert, sobald der Kern `workjet_projects`
  und `workjet_working_copies` an die Shell **exportiert**. Heute exportiert das ctox-Manifest `workjet_projects`
  (read), **nicht** `workjet_working_copies` (`v4/manifests/ctox.manifest.json` exports). Im Stub fällt das nicht auf
  (Stub prüft keine exports) → Kern-Lücke K-WJ-1.
- project.presentation.* liest Dateibereiche (`desktop_files` + `demandFileLoader.fetchFile(fileId,{range})`).
  v4 hat `files.get({id|ref})` ohne Bereich (Text ≤ 512 KiB, sonst Bytes) → Kern-Lücke K-WJ-2 (Bereichslesen oder
  BlobRef-Teilabruf), bis dahin unsupported.

### B – Supervisor turn und route, plan.step.retry → `actions/supervisor.js`
Aktionen: project.supervisor.bind, project.supervisor.turn.submit|input|cancel|watch|capabilities,
project.supervisor.route.read.v1|v2, project.supervisor.route.capabilities.v1|v2, ctox.plan.step.retry.
Orakel: `workjet-project-control.test.mjs` (Supervisor-Blöcke), `workjet-supervisor-route-native.test.mjs`,
`workjet-supervisor-*-contract.test.mjs`, `workjet-worker-outcome-contract.test.mjs`.
Kernabhängigkeiten: alles über `commandBus.dispatch` (kein requestNative). turn.watch nutzt `until:'accepted'` und
Folgezustände – v2-compat bildet `accepted` auf den Befehlsstatus ab; Zwischenstände (running) kommen nur, wenn der
Kern `result` während `running` setzt. ctox.plan.step.retry: Workjet erwartet bei fehlendem Kern unsupported/
core_outdated (`apps/web/src/lib/workjetPlanStepRetry.ts`) – passt zu unknown_command → unsupported.

### C – Jour fixe, Kalender, Sprache → `actions/meeting.js`
Aktionen: project.jour_fixe.meeting.read|start|end, project.jour_fixe.todos.revise|confirm,
project.jour_fixe.comment.add, project.jour_fixe.transcript.append, project.jour_fixe.speech,
project.jour_fixe.narration.read, project.calendar.accounts.read|events.read, speech.dictation,
speech.settings.read|configure|key|playback|voices|check|check.transcription.
Orakel: `workjet-project-control.test.mjs` (meeting/local-final/calendar), `jour-fixe-narration.test.mjs`,
`jour-fixe-speech.test.mjs`, `workjet-calendar-native.test.mjs`, `speech-dictation.test.mjs`,
`speech-settings-control.test.mjs`.
Kernabhängigkeiten (E25, Abnahmeliste in WJB-0.md): Nativmethoden `ctox.workjet.calendar.read.v1`,
`ctox.workjet.jour_fixe.speech.v1`, `ctox.workjet.speech.dictation.v1`, `ctox.workjet.speech.settings.v1` als
Kern-Befehle. Diktat: V2 ruft je Operation (open/write/read/finish/cancel) einen eigenen Request; E25 liefert
Zwischenstände als `running` unter EINER command_id. Der Strang muss entscheiden, ob `open` einen langlebigen Befehl
startet und `read` aus dem Zwischenstandspuffer bedient (v2-compat liefert `opts.onPartial`) – offene Frage an den
Kern (K-WJ-3). narration.read braucht wie Präsentation das Bereichslesen (K-WJ-2). Audio als BlobRef (files.ref).

### D – Grok, Provider, Rechner, Sitzung, Gerät/Host → `actions/instance.js`, `actions/computer.js`, `actions/session.js`, `actions/device.js`, `host-bridge.js`
(D ist der einzige Strang mit mehreren Dateien, weil vier Globals an ihm hängen; die Dateien gehören nur D.)
Aktionen: instance.grok.* (6), instance.providers.* (9), computer.* (6), session.* (6) + workjetSessionEvents,
binding.list|revoke, invite.create|revoke; Port von `mobile-host.js` (V2-DOM `.shell-window`, `is-focused`,
`data-workjet-mobile-*` → v4-Rahmen-DOM).
Orakel: `workjet-grok-native.test.mjs`, `workjet-provider-native.test.mjs`, `workjet-computer-control.test.mjs`,
`workjet-session-control.test.mjs`, `workjet-session-events.test.mjs`, `scripts/workjet-command-native.browser.mjs`
(Browserprobe, braucht eigenes Harness).
Kernabhängigkeiten: `ctox.workjet.grok.v1`, `ctox.workjet.device.v1` als Kern-Befehle (E25); providers über
commandBus. Sitzungsereignisse lesen `workjet_session_transfers` live (`collection.$.subscribe` – v2-compat liefert
das über `api.query(...).subscribe`); Export an die Shell nötig. Wichtig für das Einhängen: Workjet erkennt
„nicht unterstützt“ je Brücke verschieden (siehe WJB-0.md, Abschnitt Fehlerform) – Rechner nur über die Nachricht
`workjet_computers collection is not registered.`, Sitzung gar nicht (Global fehlt). Deshalb hängt `index.js` eine
Brücke erst ein, wenn sie mindestens eine freigeschaltete Aktion hat; Teilportierung der Sitzungsbrücke lässt
Workjet bei den fehlenden Aktionen „fehlgeschlagen“ statt „nicht unterstützt“ sehen → D portiert die Sitzungsbrücke
nur vollständig.

## Kritischer Pfad

WJB-0 (Gerüst, dieser Stand) → A, B, C, D parallel → K-WJ-1 (exports) und E25-Routen im Kern blockieren A/C/D-Teile
→ gemeinsamer Lauf `node --test 'v4/shell/workjet/tests/*.test.mjs'` mit allen Orakeln → Cutover M7 nur, wenn
`supported()` je Brücke = `INVENTORY` (Testfall dafür ergänzt der letzte Strang).
