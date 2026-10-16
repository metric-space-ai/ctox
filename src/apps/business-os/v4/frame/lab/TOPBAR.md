# Strang TOPBAR: Kopfleiste der Shell-V2 im v4-Rahmen (15.10.2026, 21:12–22:00)

Auftrag: Die Kopfleiste der Shell-V2 bleibt erhalten (Owner: „das bleibt natürlich alles erhalten!!!!“). Dazu kommt
das Zahnrad „Einstellungen“ zurück (M-20). Ursache von M-20 laut git: `8434a3c61` (07.10.) hat
`.icon-button.app-menu-button[data-open-settings]` neben dem Konto-Knopf entfernt. `d22ef2adf` (08.10.) hat den
Ersatzeintrag im Instanz-Panel in „Darstellung und Sprache“ umbenannt.

## Umgesetzt

| Punkt | Umsetzung | Dateien |
|---|---|---|
| Leiste = Shell-V2 | DOM und Klassen wie `index.html` `header.topbar`. Links stehen das Launcher-Raster (`.start-button`) und der Workspace-Umschalter mit Punkt und `⌄`. In der Mitte liegen die App-Reiter `.module-tabs` mit dem Überlauf „+N“. Den Algorithmus `chooseVisibleTopbarApps` importiert die Leiste aus `shared/topbar-apps.js`, das Steuerobjekt ist eigen, weil es aufgeräumt werden muss. Rechts folgen Zustandspunkt `.shell-sync-status`, Zahnrad und Avatar `.account-button` mit Initialen. | `v4/frame/topbar.js` (neu) |
| CSS wörtlich | Ein Generator übernimmt die Regeln, die ein Element der Leiste treffen, aus den Stylesheets von `index.html` (ui-contract, app.css, mobile-host.css, shared/shell-topbar.css). Deklarationen, @media und Reihenfolge bleiben Zeichen für Zeichen gleich. Die Selektoren werden mechanisch auf die Hülle `[data-v4-topbar]` begrenzt, die Spezifität bleibt dabei gleich: `X` → `:where([data-v4-topbar]) X`, `:root[data-shell-style=…]` → `[data-v4-topbar][data-shell-style=…]`. Sonst träfen `.drawer-body` und `.text-button` auch die Apps. Die Tokens kommen transitiv aus `:root`/`html`/`body` an die Hülle. Ergebnis: 233 Regeln, 62 Tokens, `@keyframes topbar-sync-spin`. Eigene v4-Zeilen gibt es nur in einem markierten Block am Ende: Rasterzeile, `line-height: normal` (`.v4f` vererbt 13px/1.35) und die rote Fehlerfarbe des Punkts. | `shared/shell-v2-topbar.css` (erzeugt), `v4/frame/lab/extract-topbar-css.mjs` (neu) |
| Einhängen | `import { createTopbar }`. Die CSS steht im atomar getauschten Satz nach `frame.css`, weil `.v4f :where(button) { color: inherit }` sonst die Reiterfarbe überschreibt. Ein Block nach `createTaskbar` erzeugt die Leiste. Die Reiter rendern über `taskbar.render` mit, damit es eine Quelle für Taskleiste und Reiter gibt. | `v4/frame/index.js` (3 gezielte Stellen, siehe Abweichungen) |
| Zahnrad + Einstellungen | Das Zahnrad (`.icon-button.app-menu-button[data-open-settings]`, wie vor `8434a3c61`) sitzt links vom Avatar. Es öffnet das Blatt „Einstellungen“ (`.drawer.drawer-right`, Klassen `settings-preferences` und `header-select` wie Shell-V2) mit Sprache (DE/EN) und Schema (System/Hell/Dunkel). Das Schema wirkt sofort. Beide Werte stehen in `shell_preferences.settings` (das Schema erlaubt `settings` als freies Objekt). Esc und Hintergrund-Klick schließen das Blatt, der Fokus geht zurück. | `topbar.js` |
| Konto-Blatt | Das Blatt ist `.drawer.drawer-right.account-popover` > `.account-drawer` wie Shell-V2. Ganz oben steht „Einstellungen“, darunter „Konto“ mit Name · Rolle, Rollenkarte und „Abmelden“. Rollenkarte und „Abmelden“ erscheinen nur, wenn der Kernel `actor.role` bzw. `identity.logout` liefert. | `topbar.js` |
| Workspace | `identity.instance_name`/`workspace_name`/`display_name`, sonst die Instanz-ID. `local` und `lab` werden als „Lokaler Workspace“ angezeigt (wie welsch). Der Punkt bleibt `data-health="unknown"` (grau), weil der Kernel keine Health-Quelle liefert. Ein Klick öffnet `.shell-release-panel` mit der Instanz-ID und „Einstellungen“. | `topbar.js` |
| Zustandspunkt | Die Quelle ist `currentSyncState()` aus `v4/shell/sync-state.js` (ZONE45). Ist die Quelle beim Start noch nicht da, wird einmal je Sekunde gesucht, bis sie gefunden ist. Abbildung: `ok` → `healthy` (grün, ohne Text, wie welsch), `warn` → `unconfirmed` (gelb, `SYNC_TEXT.warn`), `error` → rot (`SYNC_TEXT.error`). Bei `unknown` und ohne Quelle gibt es kein Attribut: der Punkt bleibt grau und ohne Text. | `topbar.js` |
| Reiter vs. Taskleiste | Shell-V2 (welsch, 1280 und 420 gemessen) hat **nur** die Reiter oben, keine Taskleiste unten. v4 hat nach FR6b (M-07) die Taskleiste unten. **Abweichung:** Die Reiter oben sind nach Shell-V2 gebaut, die FR6b-Taskleiste bleibt stehen. Ob beide bleiben, entscheidet der Owner (OWNER-Karte unten). | – |

## Rahmen-Labor (Fälle „TOP“, eigene Seite, `TOP_ONLY=1` für schnelle Läufe)

- TOP-0 Aufbau: Leiste 48 px oben, Desk ab 48, je offener App ein Reiter (der vorderste markiert), Reihenfolge rechts Punkt → Zahnrad → Avatar. Ein Reiter-Klick fokussiert das Fenster. Das Launcher-Raster blendet die Fenster aus und stellt sie wieder her.
- TOP-1 G0: Dasselbe Leisten-DOM wird im v4-Rahmen und mit den Stylesheets von `index.html` (app.js-Pfad) gerendert, hell und dunkel bei 1280 sowie dunkel bei 420. Ergebnis: **100 %** gleich je Drittel. Mutation (Leiste 40 px eingerückt): 84 %.
- TOP-2: Das Zahnrad öffnet „Einstellungen“ mit Sprache und Schema. „Dunkel“ wirkt sofort und steht in `shell_preferences.settings`. Esc schließt, der Fokus geht zurück. Mutation (ohne `data-open-settings`): öffnet nichts.
- TOP-3: Im Konto-Blatt ist „Einstellungen“ der oberste Eintrag und öffnet die Einstellungen. Mutation (Eintrag versteckt): oben steht „Konto“.
- TOP-4: Isoliert gilt: ohne Quelle und bei `unknown` grau ohne Text, nur `ok` grün. Im Rahmen gilt grün ⇔ `getOverallState().level === 'ok'`. Mit V4-Metriken (stub 74137b1): Rückstand 3 → gelb „Verbindung wird wiederhergestellt“, Rückstand 0 → grün. Mutation (Quelle `unknown`, Punkt von Hand auf `healthy`): wird erkannt.
- TOP-5: Bei 420 px läuft die Leiste nicht über, alle Bedienelemente sind sichtbar, das Einstellungsblatt passt.

Fachlich angepasste Erwartungen in Fremdfällen: Der Satz Shell-CSS hat jetzt 5 statt 4 Links („Shell-Tausch“, „Doppelpuffer“, „Theme-Schnittstelle“ mit Reihenfolge). FR6a G0: Die Referenz wird jetzt **mit der echten Shell-V2-Kopfleiste** aufgenommen, die Fensterebene liegt am selben Ursprung wie im Rahmen (Ebene ab 48 px). Das Studio-Fenster liegt bei y 200 statt 260, damit die Eckgriffe im 48 px niedrigeren Desk ganz sichtbar bleiben. Die Schwelle ist unverändert.

Gefundener eigener Fehler, behoben: `destroy()` entfernte `data-v4-has-topbar` am abtretenden Rahmen. Dadurch sprang dessen Raster mitten im Doppelpuffer-Tausch, die Pixelprobe lag bei 27,55 %. Jetzt bleibt das Attribut stehen.

## Abnahme (selbst ausgeführt)

Siehe Abschnitt „Endlauf“ unten. Screenshots angesehen:
- `shots/integrated-top-1280.png`, `integrated-top-420.png`, `integrated-top-settings-1280.png`, `integrated-top-settings-420.png`, `integrated-top-account-1280.png`
- Gegenüberstellung mit welsch (oben welsch, unten v4, dunkel): `integrated-top-vs-welsch-1280.png`, `integrated-top-vs-welsch-420.png`

## Abweichungen und Offenes

- **OWNER:** Die Reiter oben (Shell-V2) und die Taskleiste unten (FR6b, M-07) stehen jetzt beide. Shell-V2 hat nur die Reiter.
- **OWNER:** Bei 420 px braucht das Zahnrad (30 px) den Platz, den auf welsch der aktive Reiter hat. Bei zwei Lab-Apps zeigt v4 nur „+2“, welsch zeigt „Threads +9“. Mögliche Lösung: unter 560 px das Zahnrad ausblenden (die Einstellungen bleiben über den Avatar erreichbar). Nicht umgesetzt, weil M-20 das Zahnrad ausdrücklich verlangt.
- Der Kategorie-Akzent je Reiter (welsch: `data-workjet-category` plus `--shell-category-*`, sichtbare Fläche am aktiven Reiter) fehlt. v4-Apps tragen `layout.category` als freien Text, die Zuordnung zum Workjet-Katalog gibt es im Rahmen nicht.
- Konto-Blatt: „Anzeigename speichern“ und „Passwort ändern“ aus Shell-V2 fehlen. Der v4-Kernel hat dafür keine Schnittstelle (nur `actor.display_name`/`role`, `identity.logout`).
- Sprache: Die Wahl wird gespeichert und setzt `html.lang`. Der Rahmen liest beim Start aber `kernel.locale` vor `html.lang`. Damit die gespeicherte Wahl beim nächsten Start gilt, muss `index.js` `prefs.settings.locale` lesen. Das ist bewusst nicht in diesem Strang gemacht (gezielte Stellen).
- `index.js`: Statt EINES Edits sind es drei gezielte Stellen: Import, Eintrag im CSS-Satz, Block nach `createTaskbar`. Grund: ZONE45 verlangt, dass die CSS Teil des atomar getauschten Satzes ist. Ein loser Link, wie im ersten Stand, ließ fünf Fremdfälle rot werden.
- Der Workspace-Umschalter wechselt die Instanz nicht. Der Kernel liefert keine Instanzliste, nur `identity.switchInstance` im Stub.
- Kein `git` ausgeführt (nur gelesen).

## Endlauf (21:48–21:51, alle selbst ausgeführt)

- `node src/apps/business-os/v4/frame/lab/frame.test.mjs mock`: **144/144**
- `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 … frame.test.mjs stub`: **162/162**
- `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/74137b1 … frame.test.mjs stub`: **168/168** (Lauf 21:47). Danach nur TOP neu gelaufen (`TOP_ONLY=1`, 14/14), weil der Zustandstext in `results-stub.json` jetzt als Schlüssel steht (`text:warn`) statt als sichtbarer Text (Wächter-Ratchet, Hinweis ZONE45).
- Die fünf Fremdfälle, die nach dem ersten Einbau rot waren, sind wieder grün: Shell-Tausch-CSS-Satz, Theme-Schnittstelle, Doppelpuffer genau ein Rahmen, Doppelpuffer-Pixelprobe, FR6a G0.
- Zwischendurch rot und nicht von TOPBAR verursacht: AG-0, AG-6 und `v4/source/lab/coding-attrappe.js` fehlte (Agent-Strang, Datei während des Laufs in Arbeit). Einmal rot war außerdem „data-app nur am Inhalts-root“ (mock, Zeitpunkt). Im Endlauf ist alles grün.
- `node --check v4/frame/topbar.js`, `node --check v4/frame/lab/extract-topbar-css.mjs`: ok
