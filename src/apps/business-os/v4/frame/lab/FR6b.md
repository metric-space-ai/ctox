# FR6b – Taskleiste, Desktop-Knopf, Einstellungen-Knopf, Meldeschleife

> Runde 2 (unten) ersetzt: Kürzel Strg+Umschalt+B → Strg+Alt+B; Pins und Fenster getrennt → EIN Eintrag;
> Abschneiden → Überlauf „»“. Runde-1-Text bleibt als Verlauf stehen.

Stand 15.10.2026, ca. 21:10. Klon `~/Documents/ctox-shell-v4` (Zweig `shell-v4/prototype`), kein git außer Lesen.
Kern-Stub `~/.local/state/shell-v4/stub-pins/68b83a3`. Bedienkonzept Fassung 4 §2, §3a, §Taskleiste.

Geänderte bzw. neue Dateien: `v4/frame/taskbar.js` (Verhalten neu, Aussehen aus `frame.css` unverändert, Ergänzungen
als `<style data-v4-taskbar-css>` in der Leiste, weil `frame.css` FR6a gehört), `v4/shell/report.js` (neu),
`v4/frame/index.js` (nur: `windowList` um Gruppe ergänzt, `showDesktop` + eigener Tastatur-Listener, Taskleisten-
Erzeugung + `installReporter`, Import/Re-Export `reportProblem`), `v4/frame/INTEGRATION.md` (Abschnitt FR6b),
`v4/frame/lab/frame.test.mjs` (Funktion `fr6b()` + Aufruf; `FR6B_ONLY=1` lässt nur sie laufen).
`v4/frame/launcher.js` blieb unverändert (siehe Punkt 2).

## 1. M-07 Taskleiste: ein Eintrag je App

- Ein Eintrag je App statt je Fenster. Gruppe = `layout.presentation.taskbar_group`, sonst App (kein App-Sonderfall
  im Rahmen); Crew-Chat-Apps tragen `taskbar_group: 'crew'` → EIN Eintrag „Crew“.
- Mehrere Fenster: Zahl am Icon. Ein Fenster: Eintrag zeigt den Fenstertitel und verhält sich wie bisher
  (Klick fokussiert, Klick auf das vordere minimiert). Mehrere: Klick wechselt reihum, Rechtsklick (auch
  Kontextmenü-Taste) zeigt die Fensterliste an der Leiste (Pfeile, Enter, Esc, Klick daneben schließt).
- Platz: wird ein Titel kürzer als 44 px oder liefe die Leiste über, zeigt sie nur noch Icons (Titel im Tooltip,
  `aria-label` mit Anzahl). Gemessen per ResizeObserver bei jeder Größenänderung und jedem Rendern.
- Angeheftete Apps (linker Block) bleiben wie heute ein eigener Startpunkt; nur die Fensterseite gruppiert.
  Entscheidung: „Aussehen wie heute“ – ein Zusammenlegen von Pin und Fenstergruppe (Windows-Stil) hätte die Leiste
  sichtbar verändert. Offen, falls gewünscht.
- Gefundener Konflikt: Launcher-Gruppen nutzen schon `data-v4-group`; die Taskleiste nutzt deshalb
  `data-v4-task-group` (sonst wurde FR3-2 rot – gefunden im Gesamtlauf, behoben).

Labor: FR6b-T1…T5 grün (mock und stub). Screenshots angesehen:
`lab/shots/integrated-stub-fr6b-taskbar-1280.png` (Notizen, Aufgaben, Buchhaltung, „Crew“ mit 6),
`integrated-stub-fr6b-taskbar-menu-1280.png` (Fensterliste: 6 Chats über dem Eintrag),
`integrated-stub-fr6b-taskbar-420.png` (vier Icons, Crew mit 6). Gleiche Bilder ohne `stub-` aus dem mock-Lauf.

## 2. M-18 Desktop

- Der Knopf „Desktop“ (links in der Taskleiste) minimiert ALLE sichtbaren Fenster und merkt sie sich; der nächste
  Klick stellt genau diese wieder her, in derselben Stapelreihenfolge, mit demselben vordersten Fenster. Vorher
  schon minimierte Fenster bleiben unten. Ist nichts sichtbar und nichts gemerkt: Fokus in die Launcher-Suche.
- „Launcher schließen“: im v4-Rahmen IST der Launcher der Desktop (Kachelfläche hinter den Fenstern, kein eigenes
  Startmenü). Minimieren aller Fenster legt ihn frei; es gibt kein Menü, das zu schließen wäre. Deshalb keine
  Änderung an `launcher.js`. (Das Startmenü mit Fußzeile „Desktop/Einstellungen“ gibt es nur in Shell-V2, `app.js`
  `createStartMenuElement`; dort ruft „Desktop“ `openDesktop()` – nicht Teil dieses Klons/Auftrags.)
- Kürzel Strg+Alt+D (macOS: Control+Option+D; `event.code`, damit Option-Zeichen wie „∂“ nicht stören). Windows'
  Win+D und macOS' F11/Cmd+F3 sind im Browser nicht abfangbar bzw. belegt (F11 = Vollbild des Rahmens).
  Beleg „kein Browser-Standard“: FR6b-D3 schickt Strg+D, Cmd+D, Alt+D, Strg+Umschalt+D, Cmd+Alt+D,
  Cmd+Umschalt+B und Strg+B durch die Seite – keines wird abgefangen (`defaultPrevented` false), nur Strg+Alt+D.
  Strg+Alt+D selbst ist in Chrome, Edge, Firefox und Safari nicht belegt (Win/Linux/macOS); auf Windows-Tastaturen
  ist Strg+Alt = AltGr, AltGr+D erzeugt auf DE/US kein Zeichen.

Labor: FR6b-D1…D3 grün. Screenshots angesehen: `integrated-stub-fr6b-desktop-1280.png` (Desktop frei, Taskleiste
mit allen Einträgen gedimmt), `integrated-stub-fr6b-desktop-420.png`.

## 3. M-20 Einstellungen-Knopf – NICHT umgesetzt (Begründung, Vorschlag)

Recherche (`git log -S/-G`, nur lesend):
- `8434a3c61` (07.10.2026, „refactor(shell): simplify instance and app navigation“) entfernt in Shell-V2
  `src/apps/business-os/index.html` den Knopf rechts in der Kopfleiste
  `<button class="icon-button app-menu-button" data-open-settings aria-label="App-Menü öffnen">` (Hamburger rechts
  neben dem Konto-Knopf). Ersatz: „Einstellungen“ im neuen Instanz-Panel (Ausklapper am Instanznamen oben links).
- `d22ef2adf` (08.10.2026, „fix(shell): translate recovery menu appearance action“) benennt diesen Ersatz in
  „Darstellung und Sprache“ um. Seitdem steht oben rechts nur noch Zustandspunkt + Avatar; „Einstellungen“ ist nur
  über Startmenü-Fußzeile bzw. Instanz-Panel erreichbar – genau der Owner-Befund.
- `app.js` `wireShellActions` bindet weiterhin `document.querySelector('[data-open-settings]')` → `openSettingsDrawer()`.
  Ein Zahnrad-Knopf mit `data-open-settings` links vom `.account-button` in `index.html` wäre damit sofort wirksam;
  der Eintrag „Einstellungen“ oben im Konto-Panel gehört nach `shared/topbar-apps.js` bzw. dem Konto-Panel in `app.js`.

Warum nicht hier: Der Mangel liegt in Shell-V2 (`index.html`, `app.js`, `shared/topbar-apps.js`, `shell-topbar.css`) –
außerhalb des FR6b-Schreibbereichs. Der v4-Rahmen hat weder eine Kopfleiste noch einen Avatar/Konto-Panel noch eine
Einstellungsfläche (INTEGRATION.md, Kernel-/Vertragswunsch 3: „der Rahmen hat keine Einstellungsfläche“). Ein Zahnrad
ohne Ziel wäre ein toter Knopf. Entscheidung nötig (Orchestrator): (a) Shell-V2-Fix freigeben (Dateien oben,
klein), und/oder (b) für v4 eine Einstellungsfläche + Kopfleiste/Konto beauftragen.

## 4. M-06 Meldeschleife (Shell-Seite)

- `v4/shell/report.js`: `reportProblem({appId, kind:'bug'|'wish', windowId?})`, Rahmen-Re-Export in `index.js`,
  dokumentiert in `INTEGRATION.md` (Abschnitt FR6b). Blatt IM App-Fenster über das Fensterblatt von
  `api.shell.dialog` (`window-sheet.js`, FR5): zwei Schritte, weil das Blatt je Aufruf ein Feld hat (FR5-Datei,
  nicht meine) – „Was ist passiert?“ (Wunsch: „Was wünschst du dir?“) → „Weiter“, „Was hast du erwartet?“ → „Senden“.
- Angehängt: App, Titel, Version, Fenster-ID, markierter Datensatz (falls die App ihn mit `data-record-id`/
  `data-context-record-id` + `aria-selected`/`aria-current`/`.is-selected` kennzeichnet), aktuelle Fehler/Warnungen
  der Zustandszone, Fenstergröße.
- Fensterbild: weggelassen. Ohne Fremdbibliothek gibt es keinen verlässlichen Weg: `getDisplayMedia` fragt den Nutzer
  nach einer Bildschirmfreigabe, SVG-`foreignObject` verliert fremde Bilder/Canvas/Stylesheets bzw. macht die
  Zeichenfläche unlesbar. Feld `window_image: null` ist vorgesehen.
- Speichern wie „Bugs & Features“ heute (`modules/reports` liest `business_module_reports` und `ctox_bug_reports`;
  bisheriger Schreiber `shared/business-reporter.js` `persistLocalBusinessReport`): dieselben zwei Zeilen über
  `api.put` der Shell-App-API. Fehler → Zoneneintrag `shell.report` im Fenster, Erfolg still.
- Strg+Umschalt+B im vorderen App-Fenster. **Konflikt:** In Chrome/Edge (Windows/Linux) ist Strg+Umschalt+B
  „Lesezeichenleiste ein/aus“, in Firefox „Bibliothek“. Der Rahmen fängt es nur ab, wenn ein App-Fenster vorn ist.
  Vom Auftrag so verlangt; bei Bedarf Alternative wählen (z. B. Strg+Alt+B, frei).
- Nicht hier (Auftrag): Reiter „Meldungen“ in der Code-Ansicht, Agent-Beauftragung.

Labor: FR6b-R1 (Kürzel, Blatt im Fenster, beide Felder, Esc speichert nicht), FR6b-R2 (stub: beide Zeilen gespeichert,
`kind feature`, Datensatz `notes-1`, Zonenfehler `lab.sync`, kein Fensterbild; mock: Attrappe ohne `put` → genau ein
Zoneneintrag `shell.report`). Screenshots angesehen: `integrated-stub-fr6b-report-sheet-1280.png`,
`integrated-stub-fr6b-report-sheet2-1280.png`, `integrated-stub-fr6b-report-sheet-420.png`.

## Abnahme

| Lauf | Ergebnis |
|---|---|
| `CTOX_APP_API_STUB=…/68b83a3 node v4/frame/lab/frame.test.mjs mock` | **118/118** (davon 11 FR6b) |
| `CTOX_APP_API_STUB=…/68b83a3 node v4/frame/lab/frame.test.mjs stub` | **138/138** (davon 11 FR6b) |
| `FR6B_ONLY=1 … stub`, dreimal hintereinander | 11/11, 11/11, 11/11 |

Fremde rote Fälle: keine im letzten Gesamtlauf (vor der Umbenennung `data-v4-task-group` war FR3-2 durch FR6b rot,
behoben).

### Mutationsbelege (Feature absichtlich gebrochen → rot; stub, `FR6B_ONLY=1`, danach zurückgesetzt)

| Mutation | Datei | rot |
|---|---|---|
| M1 Gruppe je Fenster statt je App | taskbar.js | FR6b-T1 (Folgefehler: Lauf bricht ab, „Crew“-Eintrag fehlt) |
| M2 Klick wechselt nicht reihum | taskbar.js | FR6b-T2 |
| M3 Rechtsklick ohne Fensterliste | taskbar.js | FR6b-T3 |
| M4 kein Icon-Modus | taskbar.js | FR6b-T4 |
| M5 Desktop stellt nicht wieder her | index.js | FR6b-D1, FR6b-D2 |
| M6 Kürzel fängt auch Strg+D | index.js | FR6b-D3 |
| M7 Strg+Umschalt+B ohne Wirkung | index.js | FR6b-R1 |
| M8 nur eine Collection gespeichert | report.js | FR6b-R2 |
| M9 kein Datensatz/Zonenfehler angehängt | report.js | FR6b-R2 |

Einmal beobachtet: im Sammellauf brach M9 einmal mit „Lauf ohne Ausnahme“ ab (8/9), Wiederholung sauber 10/11 mit
FR6b-R2 rot; unmutiert drei Läufe 11/11. Ursache nicht ermittelt (Last durch parallele Sitzungen möglich).

## Offen

1. M-20: Entscheidung Shell-V2-Fix vs. v4-Einstellungsfläche (Punkt 3).
2. `v4/manifests/shell.manifest.json`: Schreibrecht der Shell auf `business_module_reports` und `ctox_bug_reports`
   fehlt (der Stub prüft es nicht, der echte Kern vermutlich schon).
3. Strg+Umschalt+B kollidiert mit der Lesezeichenleiste (Chrome/Edge Win/Linux) bzw. Bibliothek (Firefox).
4. Pins und Fenstergruppe derselben App bleiben zwei Einträge (Aussehen wie heute) – Zusammenlegen wäre Windows-Stil.
5. Fensterbild in der Meldung (nur mit Fremdbibliothek oder Nutzerfreigabe).
6. Sehr viele Gruppen (mehr Icons als Breite): die Leiste schneidet ab (`overflow: hidden` aus frame.css); ein
   Überlauf-Knopf „weitere“ fehlt.

---

# Runde 2 (15.10.2026, ca. 21:15) – Entscheidungen des Orchestrators umgesetzt

1. **Kürzel „Problem melden“ = Strg+Alt+B** (Mac: Ctrl+Opt+B, `event.code`). Strg+Umschalt+B ist frei und wird
   nicht mehr abgefangen (FR6b-D3 prüft es jetzt mit). Strg+Alt+B ist in Chrome/Edge/Firefox/Safari nicht belegt.
2. **Pin und Fenster = EIN Eintrag** (Windows): angeheftet ohne Fenster nur Icon (kein Titel, keine
   Laufend-Markierung); mit Fenstern Titel + Laufend-Markierung, ab 2 Fenstern die Zahl. Angeheftete zuerst in
   Pin-Reihenfolge, dann laufende. Ein angeheftetes Crew-Mitglied verschmilzt mit „Crew“. Anheften/Umordnen/Lösen
   per Ziehen bleibt (FR2-5 grün). `taskGroupOf(app)` ist die eine Gruppenregel (taskbar.js, auch in index.js).
3. **Überlauf „»“**: Reicht der Platz auch für reine Icons nicht, wandern die letzten Einträge hinter „»“ am Ende; die
   Liste zeigt genau diese (Icon, Titel, Zahl), Auswahl wirkt wie ein Klick auf den Eintrag. Nichts wird abgeschnitten.
   (Offen geblieben: das aktive Element kann mit hinter „»“ wandern; Windows hält es sichtbar.)
4. **Fensterbild** bleibt weg (`window_image: null`).
5. **App-Menü am Icon-Header** (FR6a fertig): „Problem melden“ (Hinweis Strg+Alt+B) und „Wunsch äußern“ rufen
   `reportProblem({appId, kind, windowId})` und stehen ALLEN Nutzern zur Verfügung; „Code ansehen“ bleibt an
   `apps.source.view`. Edit in `index.js` `attachChrome` (APP_MENU). Dazu angepasst: FR6a/FR4-5-Fall „ohne Recht kein
   ‚Code ansehen‘“ prüft jetzt gezielt `[data-v4-appmenu="code"|"app"]` statt „gar kein Menüeintrag“ (Aussage unverändert,
   die Melde-Einträge sind gewollt immer da).

Neue Labor-Fälle: FR6b-P1 (Pin + Fenster ein Eintrag, Idle = Icon, Klick öffnet im selben Eintrag), FR6b-O1 (Überlauf,
nichts abgeschnitten, „»“-Liste = versteckte Einträge, Auswahl fokussiert), FR6b-R3 (App-Menü: Melden für alle,
Code nur mit Recht; stub mit/ohne Recht, mock ohne). R1 prüft zusätzlich, dass vorher kein Blatt offen ist.

Screenshots angesehen (stub; mock gleichnamig ohne `stub-`): `integrated-stub-fr6b-taskbar-pins-1280.png` (K nur Icon,
Notizen, Crew 6, Aufgaben, Buchhaltung), `integrated-stub-fr6b-taskbar-pins-420.png`, `integrated-stub-fr6b-taskbar-overflow-1280.png`
(Ausschnitt), `integrated-stub-fr6b-taskbar-overflow-menu-1280.png` („»“-Liste: Crew 6, Aufgaben, Buchhaltung),
`integrated-stub-fr6b-appmenu-1280.png` (ohne Recht: Problem melden, Wunsch äußern, darunter Zonenmeldung), dazu die
Runde-1-Bilder neu erzeugt.

## Abnahme Runde 2

| Lauf | Ergebnis |
|---|---|
| `… frame.test.mjs mock` (Stub-Pin 68b83a3) | **121/121** (14 FR6b) |
| `… frame.test.mjs stub` | **141/141** (14 FR6b) |
| `FR6B_ONLY=1` mock / stub nach den Härtungen | 14/14 / 14/14 |

Letzter Gesamtlauf ca. 21:24 (nach Landung von TOPBAR `topbar.js` 21:21 und ZONE45 `sync-state.js`): stub **144/144**,
mock **129/130**. Fremder roter Fall (nicht angefasst): mock `G0 Shell-Treue` (Pixelvergleich Icon-Header/Ecken,
icon 0,018) – fiel erst nach der TOPBAR-Landung, die das Fenster-Chrome unter eine Kopfleiste schiebt; FR6b ändert
kein Chrome-Pixel (Melde-Einträge erscheinen erst nach Klick auf das Icon). Alle 14 FR6b-Fälle grün in beiden Modi.

### Mutationsbelege Runde 2 (stub, `FR6B_ONLY=1`, danach zurückgesetzt, Rückstände per grep 0)

| Mutation | Datei | rot |
|---|---|---|
| M1 keine Gruppen (`taskGroupOf` → null) | taskbar.js | FR6b-T1 (Folgeabbruch) |
| M2 Klick nicht reihum | taskbar.js | T2 |
| M3 Rechtsklick ohne Fensterliste | taskbar.js | T3 |
| M4 kein Icon-Modus | taskbar.js | T4 |
| M5 Desktop stellt nicht wieder her | index.js | D1, D2 |
| M6 Kürzel fängt Strg+D | index.js | D3 |
| N1 Melden wieder auf Strg+Umschalt+B | index.js | D3, R1 |
| M8 nur eine Collection | report.js | R2 |
| M9 kein Datensatz/Zonenfehler | report.js | R2 |
| N2 Pin und Fenster getrennt | taskbar.js | P1 |
| N3 angeheftet ohne Fenster mit Titel | taskbar.js | P1 |
| N4 kein Überlauf (Leiste schneidet ab) | taskbar.js | O1 |
| N5 Melden nur für Admins | index.js | R3 |
