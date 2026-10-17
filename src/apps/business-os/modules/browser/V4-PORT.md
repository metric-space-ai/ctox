# browser – Port auf App-API 4.0 (Strang UD5, Welle D)

Fortschritt (gemessen, `TZ=Europe/Berlin date`): 13:41 Start (Brief, PORTING.md, app-api.d.ts, Stub-README,
knowledge-Pilot gelesen) · 13:44 Scanner vorher 96 Stellen + CSS 186 ungescoped · 14:14 index.js portiert, Scanner 0,
schema.js gelöscht, CSS gescoped · 14:20 v4-Test 11/11 grün · 14:23 Bildschirmfotos 1280/420 angesehen, zwei
Layoutfehler behoben (s. u.) · 14:40 Abbruch (Sitzungslimit) · 15:00 Wiederaufnahme: Stub-Pin cff5ba5, Route-Startform
von outbound (`params.args`) repariert und getestet, `browser.test.mjs` auf v4 umgestellt (grün), Kicker-Regel
nachgezogen · 15:25 Bildschirmfotos neu angesehen · 15:52 Stub-Pin 9fa9948 (Brief geändert): `record_id` zusätzlich als
Befehls-Option `api.command(type, payload, {record_id})` (§16.19 Nr. 8, landet am Befehlsdokument; Test prüft es),
Tests lesen den Zustandsstrom über `kernel.status` (alter Name `statusZone` ist veralteter Alias); alles grün.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app browser` | **0 Stellen**, Exit 0 (vorher 96 Stellen, CSS 186 Selektoren ungescoped, 22 mit Shell-Vorfahr) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs browser modules/browser/index.css --check` | gescoped |
| guard-v4 | `node v4/tools/guards/guard-v4.mjs --json` | 0 Treffer in `modules/browser/` (ohne Tests); Testdateien zählen getrennt |
| v4-Test gegen Kern-Stub | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/9fa9948 node --test src/apps/business-os/modules/browser/tests/v4-api.test.mjs` | **11/11 grün** (Chromium headless 1223, createStubKernel storage:memory; 15:52, Pin 9fa9948 laut geändertem Brief; vorher 15:23 auf cff5ba5 grün) |
| Bildschirmfotos | `SHOTS=1 …v4-api.test.mjs` | `tests/shots/browser-1280.png`, `tests/shots/browser-420.png` |
| Quelltext-Wächter Start | `node modules/browser/browser-start-contract.test.mjs` | grün, auf v4-Vertrag umgestellt (api.command, onRoute, Startsperre bis Befehlsende) |
| Bestehende Suite | `node modules/browser/browser.test.mjs` | **grün**, Exit 0 (vorher 37 rote v3-Transportwächter; umgestellt, s. OFFEN-7) |

Abgedeckt vom v4-Test: mount zeigt nur eigene + gemeinsame Web-Stack-Sitzungen und das Bild aus `browser_frames` ·
`sim.push` erscheint live, Löschung verschwindet · leere Collections: kein Text (Titel, Chip, Bühne, Karten, Verläufe),
keine Zone · Start → `business_commands` `browser.session.start` liegt **sofort** lokal (kein Warten), Doppelklick ergibt
einen Start, `sim.failCommand` → genau ein Zoneneintrag `browser.command.browser.session.start` mit `ref` = Befehls-ID ·
Adresse + Enter → `browser.navigate` mit wiedererkannter Pacht, `done` räumt · Eingabe auf der Bühne → `api.put`
`browser_input_events` (keyDown/mouseDown mit Pacht), `put` → `accepted` · Beobachter-Eingabe über `api.shell.dialog`
(kind prompt), ohne Antwort kein Befehl · Route `api.shell.navigate('browser', {session_id, purpose:'web_stack_auth',
target_url})` startet die Anmeldesitzung, unbekannte ID → Zone `browser.route.unknown_session` · snapshot → structured
clone → Remount erhält Adressentwurf, Auswahl, Leiste, Abos 1 je Collection · Scraping-Band: Adapter beider Sammlungen
live, Abos nur bei offenem Band · unmount: alle 8 Abos 0, root leer, keine `pageerror`.
**Nicht** abgedeckt: Präzisionseingabe, Kontextmenü, Downloads, Auth-Assist-Knöpfe, Locale-Wechsel, Geometrie-Labor,
Hot-Plug-Skript.

## Was sich geändert hat

- **Einstieg**: `mount(root, api, {restored})` → `{unmount, snapshot}`. Zustand war schon je Montage (in `mount`); neu
  ist `env` statt `ctx` (`{api, root, actor, surfaceFocused, status, cleanups}`), alle Abos je Montage.
- **Daten**: sechs Live-Abos (`business_commands`, `browser_sessions`, `browser_tabs`, `browser_frames`,
  `browser_input_events`, `ctox_queue_tasks`) lösen das entprellte Neuzeichnen aus; `loadAndRender` liest synchron über
  `api.query(…).get()` / `api.get` mit denselben Selektoren/Limits wie der v3-Replikationsweg (Sitzungen:
  `owner_user_id ∈ actor` oder `payload.purpose = web_stack_auth`). Entfallen: `startCollection`, `restartCollection`
  (Frame-Sync-Recovery alle 12 s), `leaseCollection` (Adapter), `refreshBrowserProjections`, `startBrowserRuntimeSync`.
- **Direktkanal entfällt** (8× `requestNative('ctox.browser.live.v1')`): Sitzungsliste, Direktstart, Navigation,
  Steuerpacht, Eingabe-Warteschlange, Frame-Pump (Takt 0–1000 ms, `startDirectBrowserLive`), Tab-Live-Zustand,
  Skriptansicht. Alles geht als Befehl bzw. als Dokument; das Live-Bild siehe OFFEN-1.
- **Befehle**: `submitCommand` → `api.command(type, {...payload, record_id, client_context})`, nie warten; Status-Abo:
  `failed` → `api.status.report({level:'error', code:'browser.command.<typ>', ref})`, `done` → `clear`; `h.local`
  abgelehnt → ebenso. Startsperre (8 s) und Merkliste angeforderter Starts lösen sich mit dem Ende des Befehls
  (`onSettled`), nicht mehr mit der v3-Quittung. Steuerpacht: ein Weg über `browser.controller.*` (Handler pflegt die
  Pacht), lokale Spiegelung bis zur Projektion wie im v3-Rückfallzweig.
- **Pacht-Erneuerung ohne `setInterval`** (R12): ein einzelner Termin aus `controller_lease_expires_at_ms` des
  Live-Dokuments (75 s vor Ablauf erneuern, bei Ablauf neu holen), neu gesetzt nach jedem Neuzeichnen; dazu wie bisher
  Fokus/Sichtbarkeit.
- **Eingabe**: `writeInputEvent` schreibt `browser_input_events` per `api.put` (der v3-Replikationsweg; ohne
  `tenant_id`, weil 4.0 keine Instanz-ID an Apps gibt – der Core kennt sie). Keine eigene Warteschlange, kein Takt.
- **Zustandstexte raus** (index.js, index.html, locales): Bühnentexte („Browser-Fenster wird vorbereitet“, „… wird
  geladen“, „Browser wird gestartet“, „Öffne ein neues Browser-Fenster“, „geschlossen“), „Kein Browser-Fenster
  geoeffnet“, Status-Chip ohne Sitzung, „Noch keine Browser-Sitzung …“, Adapter laden/leer/Fehler, Skript lädt/fehlt,
  „No browser commands / No CTOX handoffs“, Erfolgs-Hinweise („Browser-Kontext wurde … uebergeben“, „Anmeldung
  bestätigt …“, „Zugangsdaten wurden eingesetzt“ – letzterer war nach dem Wegfall des Wartens falsch), das Hinweisband
  `[data-browser-notice]`. In die Zone gehen: `browser.notice` (Sperrgründe/Fehler, vorher Hinweisband),
  `browser.command.<typ>`, `browser.frame` (Laufzeitfehler, beendete Sitzung, abgelaufene Pacht – vorher Bühnentext),
  `browser.frame.render`, `browser.input.invalid`, `browser.import.invalid`, `browser.adapters.<collection>`,
  `browser.route.unknown_session`, `browser.script.unavailable`.
  Wortlaut-Anpassungen für guard-v4 (Sitzungs-Statuslabels, keine Zustandsmeldungen): „Nicht verbunden“ → „Beendet“
  (de) / „Closed“ (en, Band „Beendet“ zählt dieselben Sitzungen), en „Starting“ → „Requested“, „Connecting“ →
  „Pending“, Overlay „Wartet auf Anmeldung“ → „Anmeldung erforderlich“, „Seitenauswertung läuft“ → „… aktiv“,
  Dialog-Rückfall „wartet auf“ → „verlangt“, Startsperre „läuft bereits“ → „ist bereits aktiv“;
  `browserStartErrorMessage` ohne die v3-Transportcodes (`sync_unavailable`, `projection_delayed`, `native_unavailable`).
- **Startargumente anderer Apps (bestätigt für outbound)**: outbound ruft
  `api.shell.navigate('browser', {title, mode: 'maximized', args: browserAuthAssistLaunchArgs(…)})`
  (modules/outbound/index.js, Auth-Assist nach `settled`). Der erste Port las nur `event.params` flach – die
  Anmeldeargumente unter `params.args` wären **still verloren** gegangen. Jetzt `browserRouteArgs(params)` = `params.args`
  falls Objekt, sonst `params` (wie v3 `detail.args`). Test „Route“ prüft genau diese Startform mit allen 17 Feldern von
  `browserAuthRequestFromArgs`: `browser.session.start` mit session_id, tab_id, url, source_id, allowed_domains,
  capture_script, secret_name, auth_assist_command_id, `secret_value_in_rxdb: false`; `title`/`mode` landen nicht im
  Befehl; die Sitzung ist danach ausgewählt. Die flache Form bleibt gültig. Offen bleibt nur die Erst-Öffnung (OFFEN-9).
- **Shell**: `setTitle('Browser')`; Start-/Auswahlwunsch anderer Apps über `api.shell.onRoute` (vorher
  `eventBus 'browser:select-session'` + DOM-Ereignis `ctox-business-os-app-launch` + `ctx.args`); Eingaben (Upload-ID,
  Beobachter, HTTP-Auth-Secret, Seiten-`prompt`) über `api.shell.dialog({kind:'prompt', …})` statt `globalThis.prompt`;
  Benachrichtigungen des Imports → Zone; Sprache über `api.shell.t('browser.<key>')` mit eingebetteten Rückfalltexten,
  Locale-Wechsel live; Akteur über `api.actor` (`browserActorIds`, `actorContext`, `userSessionPrefix` lesen
  `actor.id`, die v3-Form `{user:{id,email,login}}` bleibt lesbar). Fensterfokus app-lokal (OFFEN-3).
- **Fensterzustand**: `localStorage 'ctox-browser-rail'` → `snapshot().railCollapsed`; Snapshot v1:
  `selectedSessionId, requestedSessionId, controllerLeaseId, ansicht, preciseEnabled, leftView, railCollapsed,
  addressDraft (nur bei Eingabe), preciseDraft`.
- **Imports/Markup/CSS**: `ctox:shared/pane-grammar.js`, `autoWirePaneGrammar(root)` ruft die App selbst;
  `shared/i18n.js` (HTTP-Locales), `ensureStyles`, `?v=`/`STYLE_BUILD`, `fetch(…, {cache:'no-store'})` entfallen;
  `fetch(new URL('./index.html', import.meta.url))`. index.css unter `[data-app="browser"]`, 22 Shell-Vorfahren ersetzt.
- **Layoutfehler aus den Bildschirmfotos behoben**: (1) Sitzungskarten wurden über die ganze Spaltenhöhe gestreckt
  (Grid ohne `align-content`; in v3 verdeckt durch Shell-Regeln) → `align-content: start`. (2) Die 768px-Haltepunkte
  (`@container business-app-window`) griffen nicht, weil der Container der Shell außerhalb von root liegt → root ist
  selbst `container: business-app-window / inline-size`; bei 420 stapeln Sitzungen und Bühne wie vorgesehen.
  (3) Spaltentitel schrumpfte auf einen Buchstaben, weil `.shell-window … .ctox-pane-titles > .ctox-pane-title
  {flex: 0 0 auto}` aus app.css fehlt → in index.css übernommen. (4) Wiederaufnahme 15:2x: Gemessen (Playwright,
  1280) Titelzeile 212 px, Kicker 103 px fest, Titel 74 px → „Sitzungen“ bei 1280 ganz, bei 420 bis auf „S“ verdeckt.
  Die zweite Hälfte derselben app.css-Regel (Kicker `flex: 0 1 auto; min-width: 0` – „zuerst weicht der Kicker“)
  fehlte; übernommen. Jetzt „C… Sitzungen“ (1280) bzw. „CTO… Sitzungen“ (420).
- **schema.js gelöscht**: kanonisch (Schlüssel sortiert) gleich zu `collections.schema.json` für alle 6 Collections
  (Version, `properties`, `required`, `indexes`, `primaryKey`, `additionalProperties`). **Abweichung**: schema.js trug
  Identitäts-Migrationen (`retainV0BrowserDocument = {...oldDoc}`) für browser_* v1 und
  outbound_lead_generation_adapters v1/v2; `collections.schema.json` hat keine `migration_strategies` – inhaltlich
  gleich (Kopie), im Manifest-Strang ggf. als `{operations: []}` nachtragen. `v4/tools/manifest/extra/olg-manifest.mjs`
  nennt `browser/schema.js` nur im Text (`_todo`); der Browser **liest** die Adapter nur, er schreibt sie nicht.

## Befehlstypen – Einstufung (§16.13)

Rust-Bezüge gemessen mit `grep -rn '"<typ>"' src/core --include='*.rs' | wc -l` im Klon (14:23).

| Befehlstyp | Rust | Einstufung | Stand |
|---|---:|---|---|
| `browser.session.start`, `.navigate`, `.reload`, `.back`, `.forward`, `.tab.open/.activate/.close`, `.clipboard.copy/.paste/.clear`, `.permission.respond`, `.dialog.respond`, `.credential.fill`, `.session.stop`, `web_stack.auth_assist.complete` | 12 / 11 / 2 / … / 4 / 3 | Fähigkeit `ctx.cap.browser.*` + Lease-Gate (Fachlogik) | Handler portiert (Strang H7, `V4-HANDLERS.md`) |
| `browser.controller.acquire/.renew/.release` | 4 (acquire) | Fachlogik (Pacht in `browser_sessions`) | Handler portiert (H7) |
| `ctox.browser_context.capture` | 10 | Core-generisch (Übergabe an CTOX-Queue) | bleibt; nicht in diesem Strang |
| `browser.capture.extract` | 4 | Legacy, Ziel Fähigkeit (Seite auslesen = Browser-Engine) | kein Handler – OFFEN-4 |
| `browser.upload.select`, `browser.observer.grant/.revoke`, `browser.http_auth.respond`, `browser.webauthn.respond`, `browser.download.release/.rescan/.discard` | **0** | ohne Empfänger (weder Rust noch Handler) | Knöpfe senden heute schon ins Leere – OFFEN-4 |

## OFFEN (nummeriert)

1. **Live-Bild und Eingabe (Echtzeit-Ausnahme des Kerns, unzuverlässige Spur)** – nicht gebaut. Die UI braucht eine
   nicht persistierte Spur je Sitzung, z. B. `api.live.open('browser', {session_id, lease_id}) → {frames:
   LiveDoc<{seq, mime_type, data|blob, width, height, captured_at_ms, nav?:{url,title,tabs,active_tab_id}}>,
   send(events[]) → {applied, input?:{tag,editable,valueLength}, results}, close()}` mit Pacht-Prüfung im Kern,
   Verwerfen alter Bilder (Navigations-Epoche), gedrosselter Bildrate und Zustandscodes für Ablehnung („lease missing
   or expired“, „unknown op“). Bis dahin zeigt die App nur, was der Kern als `browser_frames` projiziert, und schreibt
   Eingaben als `browser_input_events` (v3-Replikationsweg). **Risiko**: v3 hat genau diesen persistierten Bildweg
   abgeschaltet („schrieb bei jedem Takt ein Bild nach SQLite/IndexedDB“); der Kern darf `browser_frames` nicht als
   Bildstrom füllen.
2. **Skriptansicht**: v3 las das Automatisierungsskript über den Direktkanal (`op:'script'`). 4.0 hat keinen Weg; die
   App meldet `browser.script.unavailable` (info). Vorschlag: Projektion `browser_sessions.payload.script_ref` +
   `api.files.get(ref)`, oder Teil von OFFEN-1.
3. **Fensterfokus**: Die Pacht-/Eingaberegeln hängen am Fokus des Shell-Fensters (`.shell-window.is-focused`). 4.0 hat
   keine Fokus-API; die App führt den Fokus selbst (Zeiger/Fokus in root = fokussiert, außerhalb = nicht; Start =
   `document.hasFocus()`). Wunsch: `api.shell.window.focused: LiveDoc<boolean>`.
4. **Befehle ohne Empfänger**: 9 Typen (Tabelle) haben weder Rust noch Handler; `browser.capture.extract` braucht eine
   Fähigkeit. Mit Strang H7/Kern klären (Handler oder Knöpfe entfernen – Fachentscheidung, nicht hier).
5. **Fremd-Collections**: `ctox_queue_tasks`, `outbound_research_adapters` (fehlt in `module.json.collections`),
   `outbound_lead_generation_adapters` werden gelesen → im Manifest `foreign_collections` (Lesen) nötig, sonst
   `forbidden` (Stub prüft es nicht). `business_commands` ist System-Collection.
6. **Shell-Regeln außerhalb von root**: Die App hing an Shell-CSS mit `.shell-window[data-shell-contract="v2"]`-
   Vorfahr (Container `business-app-window`, Titel-Flex). Zwei Regeln hat die App übernommen; die v4-Shell muss
   klären, ob sie den Container um root legt (dann ist die App-Regel harmlos doppelt).
7. **Bestehende Suite `browser.test.mjs`** (1187 Zeilen): Import über `ctox:`-Auflöser (Node `registerHooks`)
   umgestellt; die reinen Logik-Prüfungen laufen, **37 Zusicherungen** prüfen den v3-Transport im Quelltext
   (`requestNative`, `commandBus`, `restartCollection`, `?v=`, Direkt-Frame-Pump, `ctx.host.closest('.shell-window')`)
   bzw. entfernte Test-Hooks (`directResponseBelongsToSurface`). **Erledigt 15:2x**: jede v3-Zusicherung durch die
   v4-Entsprechung ersetzt, nicht gelöscht – ein Befehlsweg (`env.api.command` genau 1×, `submitCommand` ohne
   `await`/`until`, failed → Zone, done → clear), kein Direktkanal/Sync-Kopplung (Code ohne Kommentare), Markup ohne
   Buster, Eingabe per `api.put('browser_input_events')`, kein `setInterval`/Pump, Pacht über `browser.controller.*`,
   Sitzungs-Selektor `$or` (eigene + `web_stack_auth`), Bild nur aus `browser_frames` der Auswahl, Fokus/Akteur in
   v4-Form (`env.surfaceFocused`, `env.actor`), Eingabe-/Navigationsfälle gegen eine v4-env (api.put/api.command).
   Ersetzt: Direktkanal-Hooks `directResponseBelongsToSurface`/`applyDirectNavigationState` → Test von
   `applyLatestNavigationResult` (Navigation aus dem Befehlsergebnis, richtige Sitzung). Weggefallen: die vier
   Skript-Zustandstexte in der Locale-Prüfung (`scriptNoSession/Loading/Failed/NeedsControl`, s. OFFEN-2).
   Testhook `applyLatestNavigationResult`, `browserRouteArgs` exportiert.
8. **Mehrere Fenster**: Manifest `multi_instance: false`; Zustand ist je Montage, Modulweit nur der Übersetzer `t`.
9. **Initiale Route**: `onRoute` feuert nur bei Navigation nach dem Mount (Stub cff5ba5: `navigate` ruft nur
   registrierte Listener). Ist der Browser beim Auth-Assist von outbound **noch nicht offen**, kommen die
   Startargumente heute nicht an. Vertrag nötig: Shell öffnet das Ziel und liefert `params` nach `mount` als erstes
   `onRoute` nach (bevorzugt, App unverändert), oder `mount(root, api, {restored, params})`. Gilt für jede Ziel-App.
10. **Befund aus dem Port (unverändert übernommen)**: `state.steuerungZurueckholen` wird – wie in v3 – erst nach dem
    `return` des `mount` zugewiesen und ist damit nie gesetzt; die „Übernahme durch Interaktion“ bei Eingabe ohne
    Steuerung greift deshalb nie. Fachentscheidung, ob das beabsichtigt ist.
11. Kein git benutzt; Originale lesend gesichert im Sitzungs-Scratchpad.
