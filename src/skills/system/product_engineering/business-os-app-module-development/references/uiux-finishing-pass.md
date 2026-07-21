# Business OS — UI/UX-Feinschliff (Brief für den Spezialisten)

Stand 2026-07-21. Dieses Dokument ist der Arbeitsauftrag für die letzte
Qualitätsrunde über ALLE Business-OS-Apps und die Shell. Es bündelt jedes
Learning der App-Perfektionierungs-Kampagne. Es ersetzt nicht den Skill
`business-os-app-module-development` — `references/design-guide.md` und
`references/green-checklist.md` bleiben verbindlich; dieses Dokument sagt,
worauf der Feinschliff zielt und wie er abläuft.

## 0. Mission und Haltung

Die Apps sind funktional und grammatik-konform („ok"). Der Feinschliff hebt
sie auf „fantastisch": Ein Operator, der acht Stunden am Tag darin arbeitet,
soll das System als schnell, ruhig, präzise und schön empfinden. Maßstab ist
ein natives OS, kein Web-Dashboard. Konkret heißt das: nichts springt, nichts
flackert, nichts wartet sichtbar, nichts wiederholt sich, und jede Fläche hat
eine erkennbare Hierarchie aus Typografie, Abstand und Tiefe — nicht aus
Rahmen und Farbe.

Der Feinschliff ändert KEINE Fachlogik, keine Command-Flows, keine Schemas.
Er arbeitet an Wahrnehmung, Rhythmus, Dichte, Übergängen und Konsistenz.

## 1. Die Shell zuerst: von „ok" zu „fantastisch"

Die Shell ist die höchste Priorität dieser Runde, denn jede App erbt ihre
Wirkung. Was oft wiederverwendet wird, MUSS in der Shell leben (base.css-Kit,
`shared/pane-grammar.js`, Resizer, Scroll-Guard) — niemals erneut pro App.

Prüf- und Verbesserungsfelder, in dieser Reihenfolge:

1. **Fenster-Chrome und Tiefe.** Fenster brauchen eine glaubwürdige
   Elevations-Geschichte: fokussiertes Fenster deutlich über den anderen
   (Schatten über die vorhandenen Elevations-Tokens, kein Hardcoding),
   inaktive Fenster leicht zurückgenommen (Header-Kontrast, Scrim via
   `::before` wie im ctox-Chrome-Standard). Der dynamische Schatten existiert
   bereits — kalibrieren, nicht neu erfinden.
2. **Motion.** Ein einziges, konsistentes Bewegungssystem: Fenster öffnen/
   schließen/minimieren, Snap-Preview, Start-Menü, Drawer, Toasts. Kurze
   Dauern (120–200 ms), eine gemeinsame Easing-Kurve, `prefers-reduced-motion`
   respektieren. Keine verstreuten Einzel-Animationen; was nicht ins System
   passt, fliegt raus.
3. **Typografischer Rhythmus.** Kicker/Titel/Meta/Body über alle Panes auf
   eine Skala bringen (Größe, Gewicht, letter-spacing der Uppercase-Kicker).
   Zahlen in Listen und Zählern mit `font-variant-numeric: tabular-nums`.
4. **Abstands-Rhythmus.** Ein Spacing-Raster (4/8er-Logik) konsequent in
   Headern, Filterbars, Wells, Footern. Die häufigste „nur ok"-Ursache sind
   uneinheitliche vertikale Abstände zwischen Header → Filterbar → Band →
   Well.
5. **Taskbar, Start-Menü, Dock.** Gleiche Icon-Größenlogik, gleiche
   Hover/Active-Zustände, Badge-Anatomie (Threads „N brauchen mich") überall
   identisch. Hover-Chips (Icon-Rail-Muster aus coding-agents) als
   Shell-Standard für schmale Leisten.
6. **Docking und Fenster-Gesten.** Links/rechts/oben/Ecken-Snap inkl.
   Clamp-Verhalten (Zeiger jenseits einer Inset-Grenze zählt als Kante — nie
   abbrechen). Snap-Preview sichtbar, ruhig, korrekt in beiden Themes.
7. **Zustände.** Fokus-Ringe (sichtbar, token-basiert), Selektion,
   Hover, Disabled, Drag — überall dieselbe Sprache. Leere Zustände laden zum
   ersten Schritt ein (`.ctox-empty` mit primärem Create-Weg), niemals tote
   Flächen.
8. **Beide Themes, echte Marken.** Light/Dark UND mindestens eine
   Custom-Brand-Fixture; alles muss über Tokens laufen. Ein einziger
   hartkodierter Farbwert ist ein Fund.
9. **Wahrgenommene Geschwindigkeit.** Loader sind im Normalbetrieb unsichtbar
   (OS-snappy-Regel): Skeletons nur aus den echten index.html/index.css der
   Module abgeleitet, Sync-Hinweise als Toast, nie als Layout-Verdränger.

## 2. Verbindliche Spalten-Grammatik (Kurzfassung)

Vollform im design-guide; hier die Abnahme-Anatomie jeder Listen-/Auswahlspalte:

- Header: Kicker + Titel links; ALLE Element- und Standing-Aktionen als
  gesammelte Icons oben rechts (`.ctox-pane-icon`), darunter immer Import und
  Export. Keine Text-Buttons im Content-Fluss, keine Refresh-Buttons, keine
  Dauer-Status-Badges.
- Filterbar: Suche + kanonischer Shard/Listen-Umschalter (gestapelte
  Rechtecke / drei Linien, IN der Filterbar) + Tray-Toggle mit Aktiv-Punkt.
- Eingeklappter Filter-Tray: Dropdowns + Zurücksetzen; Scope ist ein Filter,
  keine Dauerzeile.
- Gezähltes View-Band: ≥ 2 ECHTE Ansichten „Name (n)", Nullen inklusive; ein
  einzelner Tab ist verboten (Zähler gehört dann in den Footer).
- Recessed Well (color-mix-Vertiefung) unter einer Divider-Linie; einzeiliger
  Footer.
- Shards sind reine Selektoren: Titel + EINE Meta-Zeile; keine
  Inline-Expansion. Inhalte navigiert man in der Hauptansicht.
- Chrome kommt von der Shell: `data-pg-*`-Markup + Kit-Klassen; die Shell
  verdrahtet (`autoWirePaneGrammar`), das Modul hört auf
  `ctox-pane-grammar-change` und nutzt `pane.__ctoxPaneGrammar`
  (setCounts/setFooter, null-guarded). Per-App-Chrome-CSS/JS ist ein Fund.

## 3. Die Interaktions-Gesetze (Re-Renders bewegen den Operator NIE)

Die teuersten Fehler dieser Kampagne waren Interaktionsfehler, die in zwei
Sekunden Handtest auffallen:

1. **Auswahl ist ein In-Place-Flip.** Klick auf ein Listenelement togglet
   `is-selected`/`aria-selected` über bestehende Zeilen — NIE ein Rebuild der
   Liste (`innerHTML=''` klemmt scrollTop auf 0). Der Shell-Scroll-Guard
   (`guardPaneScroll`, automatisch auf jedem `data-pg`-Pane) ist Sicherheitsnetz,
   keine Erlaubnis.
2. **Daten-Re-Renders erhalten Scroll UND Fokus.** Nur der Listeninhalt im
   Well wird neu gebaut; Header/Filterbar/Suchfeld-Node werden bei Refreshes
   nie ersetzt (sonst Fokus-Verlust beim Tippen). Gewollte Resets (Suche/
   View/Band/Filter) bleiben gewollt.
3. **Auto-Reveal.** Ein default-verstecktes Detail-/Inspector-Pane zeigt sich
   bei Selektion (`visible = hasSelection && !userCollapsed`) und lässt eine
   Selektion nie ins Leere laufen. Default-hidden OHNE Auto-Reveal ist eine
   UX-Regression.
4. **Ein-Klick-Domänenpfade.** Buchen/Claimen/Freigeben in Kalender-/Slot-
   Domänen funktioniert in einem Klick aus der sichtbaren Fläche, mit echten
   Konfliktregeln.
5. **Rechtsklick trägt den Record.** Jede Zeile/Karte/Node exponiert
   `data-context-record-id/-type/-label`, damit „Chat to CTOX" das Ziel erhält.

## 4. Technische Fallen (jede hat uns real getroffen)

- Modul-CSS UND Modul-Markup erben den JS-Cache-Buster (`?v=` aus
  `import.meta.url`); Version dreiteiliges Semver.
- Explizite Pane-Grid-Rows (`auto [auto] minmax(0,1fr) auto`) + grid-column-
  Pins; die Primärspalte behält ein hartes Minimum (Seiten-Maxima hungern
  sonst die 1fr-Mitte aus).
- `srcdoc`-iframes: nur bei Inhaltswechsel zuweisen + Load-Watchdog (erste
  Zuweisung kann beim Mount verschluckt werden).
- Lazy geladene Vendor-Singletons single-flight halten (ein Promise im
  State), sonst konkurrierende Instanzen.
- `[hidden]` muss gegen Klassen-`display` gewinnen.
- Kein `localStorage`; UI-Zustand über `ctx.storageScope`.
- `business_commands` (und andere Shell-Collections) definiert ein
  Store-Modul NIE in schema.js/collections.schema.json — Zugriff wird nur in
  module.json deklariert.
- Dritte Spalte nur mit `layout.third_pane_justification`; die meisten Apps
  haben keine.

## 5. Ablauf des Feinschliffs (pro App, im echten Shell)

Reihenfolge: Shell zuerst (Abschnitt 1), dann Referenz-Apps (knowledge,
threads, coding-agents, app-store, reports, ctox), dann alle übrigen nach
IA-Karte. Pro App:

1. App im echten Business-OS-Shell öffnen (kein Standalone-Preview).
2. **Interaktions-Proof:** Liste füllen/scrollen → unteres Element wählen →
   Offset bleibt, Auswahl sitzt; während Tipp-Eingabe in der Suche einen
   Daten-Refresh abwarten → Fokus und Eingabe bleiben.
3. **Grammatik-Sicht:** Header-Icons vollständig (inkl. Import/Export),
   Band gezählt mit Nullen, Tray eingeklappt mit Aktiv-Punkt, Footer
   einzeilig.
4. **Dichte und Rhythmus:** Abstände aufs Raster, Typo-Skala, tabellarische
   Ziffern, keine leeren „not set"-Karten, Sektionen nur mit Daten.
5. **Themes und Breiten:** Light/Dark, Desktop + Fenster-Minimum + 360px
   (Mobile-Sheet), eine Custom-Brand-Fixture.
6. **Leere und erste Schritte:** Empty-State erzeugt den ersten echten
   Record; Create-Flow real durchklicken.
7. **Motion:** Öffnen/Schließen/Drawer/Toasts im Shell-Bewegungssystem;
   nichts ruckt, nichts blockiert.
8. Befunde sofort fixen (IA/Chrome/CSS), Fachlogik-Funde nur notieren.

## 6. Prozessregeln (unverändert verbindlich)

- Preflight in der Session: design-guide + green-checklist + pane-grammar-
  Snippet + `shared/pane-grammar.js` + eine Referenz-App LESEN, bevor
  irgendeine Fläche angefasst wird.
- Harte Datei-Whitelist pro App (Modulordner; Shell-Arbeit: base.css,
  pane-grammar.js, app.js-Chrome — nichts Natives, keine Schemas).
- Tests erweitern, nie abschwächen; Abnahme = `node --check`, Modul-Tests,
  `module_static_check.mjs <id>` — alle grün, Output vollständig zeigen.
- Escape-Hatch: Scope-Bedarf außerhalb der Whitelist → STOPP + BLOCKED-Datei,
  nie eigenmächtig verbreitern.
- Report-Tail: Was geändert / Befunde / Validierungs-Output / Pfade.
- Ein Commit pro App bzw. pro Shell-Baustein; nach Landung spiegelt der
  Orchestrator ins laufende Release (rsync + APP_BUILD-Stempel) und macht den
  Browser-Proof.

## 7. Abnahme-Definition („fantastisch")

Eine App gilt als fertig geschliffen, wenn (a) alle Proofs aus Abschnitt 5
im echten Shell bestanden sind, (b) kein Punkt der green-checklist offen ist,
(c) sie in beiden Themes und drei Breiten ruhig und hierarchisch wirkt, und
(d) zwei Sekunden ziellosen Herumklickens keinen Sprung, Flacker- oder
Fokusfehler provozieren. Die Shell gilt als fertig, wenn ein frisches
Fenster-Ensemble (öffnen, snappen, resizen, minimieren, Theme-Wechsel) wie
aus einem Guss wirkt — Bewegung, Tiefe und Typografie aus einem System.
