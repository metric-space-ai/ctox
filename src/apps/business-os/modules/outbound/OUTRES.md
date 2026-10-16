# OUTRES – outbound „…“-Menü und research toter Code (TEXT-E, offene Fragen 2 und 4)

Pfade relativ zu `src/apps/business-os/`. Zeilennummern = Stand vor der Änderung. Laufender Bericht, nach jedem
Schritt fortgeschrieben.

## Schritt 0 – Ausgangslage (16.10.2026, 08:12)

- Wächter `node v4/tools/guards/textdiaet-rule.mjs --summary`: 33 Treffer, research 16, outbound 6.
- Tests vorher (Stub-Pin 68b83a3):
  - `modules/outbound/tests/v4-api.test.mjs` + `modules/outbound/outbound.test.mjs` (importiert focus-navigation,
    refresh): 36/36 grün.
  - `modules/outbound/handlers/tests/*.test.mjs`: 163/166 grün, **2 rot schon vorher** (Umgebung, nicht OUTRES):
    `legacy-forwarders.test.mjs:44` „every handler file exports a default async function“
    (`ERR_UNSUPPORTED_ESM_URL_SCHEME`, Schema `ctox:` ohne Loader) und `outbound.automation.sim.test.mjs` (Datei).
  - `modules/research/tests/*.test.mjs` + `modules/research/test.mjs`: 139/139 grün.
- Screenshots vorher (Rundgang `v4/tools/rundgang/rundgang2.mjs --max-steps 3`, Stub-Pin 637caf6, 1280 + 420):
  `modules/outbound/tests/shots/OUTRES/before/outbound/`, `modules/research/tests/shots/OUTRES/before/research/`.
  Zusätzlich Tabellenansicht mit Firmenzeilen über den Menü-Rundgang (gleicher Server/Rahmen wie rundgang2,
  Hilfsskript `tests/shots/OUTRES/menuwalk-*.mjs`, s. Schritt 3): `before/outbound/{1280,420}-02-table.png`, Befund `menuwalk-before.json`.
- Vorher sichtbar je Firmenzeile (Fixture): `company-005` (offen) 6 Knöpfe „Qualifizieren, Ausschließen,
  Ausblenden, Nachrecherche, Validiert, Log“; `company-011` (ausgeschlossen) 4 Knöpfe. Je Kampagne 4 Symbole
  „Importjob anlegen, Kampagnen-Runbook öffnen, Kampagne bearbeiten, Kampagne löschen“.

## Schritt 1 – outbound: eine Hauptaktion + „…“-Menü (08:14–08:22)

Menü-Muster: kein neues Menü. Die App ruft den Rahmen-Standard `api.shell.menu({items, at, label})` auf
(`v4/frame/window-sheet.js:284`, FR5): Menü im Fenster, Rolle `menu`/`menuitem`, Pfeiltasten, Pos1/Ende,
Enter/Leertaste wählt, Esc schließt und gibt den Fokus an den Knopf zurück. Gleiche Form wie invoices
(`openMoreMenu`, `index.js:1108`) und explorer (`showRowActions`); Symbol = die drei Punkte aus invoices
`index.html:33`. Der Auslöser ist ein echter `<button aria-haspopup="menu" aria-expanded>` → Enter/Leertaste öffnen
nativ. `shared/context-menu.js` (Shell-eigenes Menü der v2-Shell) ist für v4-Apps nicht vorgesehen (iot V4-PORT:
`createContextMenu` → `api.shell.menu`).

Änderungen `modules/outbound/index.js` (Zeilen = Stand nachher):
- `renderCompanyRowActions` (`:3957`) ersetzt die zwei wortgleichen Knopfblöcke beider Tabellenansichten
  (vorher `:3621-3637` und `:3695-3711`). Sichtbar je Zeile: Status-Badge (nur qualifiziert/ausgeschlossen, wie
  vorher), höchstens EINE Hauptaktion, „…“.
  - Hauptaktion (`companyPrimaryAction`): offene Firma → „Qualifizieren“; qualifiziert, nicht in Pipeline →
    „In Pipeline“; sonst keine.
  - „…“ (`companyMenuItems`): Nachrecherchieren, Als validiert markieren, Run-Log herunterladen, Ausblenden,
    Trennlinie, Ausschließen (rot, nur offene Firma). Menü-ids = bisherige `data-action`-Werte.
  - Beschriftungen im Menü als Verben (vorher Knopf „Nachrecherche“/„Validiert“/„Log“ mit Tooltip
    „Unternehmensdaten nachrecherchieren“ / „Recherche nach menschlicher Kontrolle markieren“ /
    „Agent-Run-Log herunterladen“).
- **Bewusste Verhaltensänderung** `isCompanyOpen` (`:3931`): „offen“ = weder `qualified` noch `rejected`
  (wie `labelQualification` „neu“). Vorher galt nur `pending` als offen. Der Import setzt aber `input`
  (`handlers/outbound.source.import.js:155`), die Normalisierung `new` (`:1530`): solche Firmen zeigten vorher
  fälschlich das rote Badge „Ausgeschlossen“ und hatten weder Qualifizieren noch Ausschließen. Jetzt: im Eingang
  „Qualifizieren“ als Hauptaktion (wie im Auftrag gefordert), Ausschließen im Menü, kein falsches Badge.
- Kampagne (`renderCampaignItem`, `:2859`): sichtbar „Bearbeiten“ (✎, sonst nirgends erreichbar, häufigste
  Aktion) + „…“. Im Menü (`campaignMenuItems`): Importjob anlegen (auch im Kopf der Mitte für die gewählte
  Kampagne), Kampagnen-Runbook öffnen (ausgegraut ohne Runbook, wie vorher `disabled`), Trennlinie, Kampagne löschen
  (rot; destruktiv gehört nach Standard ins Menü, fragt weiter über `api.shell.dialog`).
- Klickpfad: `runCompanyAction` / `runCampaignAction` bündeln die bisherigen Zweige des Klick-Listeners; Knopf und
  Menü laufen über denselben Code (`:2087-2140`, `hide-company` `:2282`). `openActionMenu` setzt `aria-expanded`.
- `index.css`: `.outbound-cell-actions` `align-items:center`; `.outbound-more-button` 26 × 26 wie die
  Kampagnen-Symbole.

Tests `modules/outbound/tests/v4-api.test.mjs`:
- angepasst „FR5-1 Rahmen-Dialog … Kampagne löschen“: Löschen liegt jetzt im „…“-Menü → Test öffnet „…“, wählt
  `delete-campaign`, Rest (Blatt im Fenster, rot, Enter löscht) unverändert. Grund: Symbol absichtlich ins Menü.
- neu „OUTRES: Firmenzeile eine Hauptaktion + „…“-Menü …“: offene Firma = [Qualifizieren, …], qualifizierte =
  [In Pipeline, …]; Enter am „…“ öffnet, erster Eintrag fokussiert, Pfeil runter, Esc schließt + Fokus zurück;
  Maus: Ausschließen über das Menü patcht `qualification_status=rejected`; Kampagne = [Bearbeiten, …], Menü
  [Import, Runbook, Löschen].

## Schritt 2 – research: toter Code entfernt (08:22–08:24)

Beleg „tot“ vor dem Löschen (`git grep -n "DRONE_SOURCES_METADATA\|computedDecisionNotes\|decisionNote" -- .`):
- `DRONE_SOURCES_METADATA` (`index.js:220-367`, 148 Zeilen, Drohnen-/Militär-Musterkatalog): einziger Treffer ist
  die Definition. Kein `export` (Exporte nur `mount` `:454` und `__researchTestHooks` `:6143`, dort nicht
  enthalten), kein `eval`/`new Function`/`window[...]`/`globalThis[...]` im Modul, einziger dynamischer Import ist
  `research-graph.mjs`. Keine Tests, Handler, v4-Werkzeuge greifen darauf zu (nur TEXT-B/TEXT-E-Berichte nennen ihn).
- `computedDecisionNotes` (`index.js:3940-3961`): nur die Definition, kein Aufrufer, nicht in den Test-Hooks.
  Die Schlüssel `decisionNoteEv1/Quant/Gap/Scope` (+`Body`) und `decisionNotes` („Entscheidungsnotizen“) kommen nur
  dort bzw. nirgends vor (kein `data-i18n`, kein zusammengesetzter Schlüssel) → in `locales/{de,en}.json` je
  9 Schlüssel entfernt. `test.mjs:1089` prüft nur `defaultCriteriaText`/`defaultPromptText` – unberührt.
- Danach: `node --check` OK, Tests 139/139 grün, Rundgang identisch (1280: 6 Schritte/0 Schreib, 420: 8/0),
  Bild vorher/nachher selbst verglichen: gleich.

Wächter `node v4/tools/guards/textdiaet-rule.mjs --summary`: **33 → 21** gesamt, research **16 → 4**
(12 weniger: 9 Code-Treffer laut TEXT-E plus 3 aus den entfernten Locale-Schlüsseln). outbound bleibt 6 (die
neuen Menütexte lösen nichts aus). Rest research = TEXT-E Frage 5 (`index.js:3454/3457` Tab-Tooltips,
`defaultCriteriaText` de/en).

## Schritt 3 – Nachweis „jede Aktion erreichbar“ (echte Maus + Tastatur, 1280 und 420)

Werkzeug: `rundgang2.mjs` (Server, Stub 637caf6, echter v4-Rahmen, Start per Desktop-Symbol, Maximieren) als Kopie
mit gezielten Schritten statt der Kandidaten-Schleife: `tests/shots/OUTRES/menuwalk-gen.mjs` +
`menuwalk-body.mjs` (`node menuwalk-gen.mjs && node menuwalk.mjs --mode walk|before --out …`; einzige Ergänzung:
`api.shell.navigate` wird mitgeschrieben). Klicks per `page.mouse.click` auf die Knopfmitte, Wirkung gemessen an
Schreibvorgängen der App, Downloads, Blatt, Navigation. Ergebnis `after/outbound/menuwalk-walk.json` und
`menuwalk-after.log`; beide Breiten gleich, 0 Seitenfehler, 0 Abbrüche.

Klicks ab sichtbarer Tabelle bzw. Kampagnenliste (bei 420 zusätzlich wie vorher 1 Klick „Kampagne öffnen“, um von
der Liste in die Tabelle zu kommen):

| Aktion | vorher | nachher | Wirkung im Rundgang (1280 und 420) |
|---|---|---|---|
| Qualifizieren (offene Firma) | 1 | 1 (Hauptaktion) | `patch outbound_companies qualification_status=qualified` |
| In Pipeline (qualifiziert) | 1 | 1 (Hauptaktion) | `patch … pipeline_status=pipeline` |
| Ausschließen (offene Firma) | 1 | 2 („…“ → Ausschließen) | `patch … qualification_status=rejected` |
| Nachrecherche | 1 | 2 („…“ → Nachrecherchieren) | `command outbound.company.research` + `put outbound_research_runs` |
| Validiert | 1 | 2 („…“ → Als validiert markieren) | `patch … research_validation_status=validated` |
| Log | 1 | 2 („…“ → Run-Log herunterladen) | Download `wagner-elektrotechnik-gmbh-research-log.json` |
| Ausblenden | 1 | 2 („…“ → Ausblenden) | Zeilen 2 → 1 |
| Kampagne bearbeiten | 1 | 1 (Symbol ✎) | Editor offen (`save-campaign-edit` sichtbar) |
| Importjob anlegen | 1 | 2 („…“ → Importjob anlegen); für die gewählte Kampagne weiter 1 im Kopf | Import-Schublade offen, Schließen per × |
| Kampagnen-Runbook öffnen | 1 | 2 („…“ → Runbook) | `navigate knowledge {id: runbook:…campaign-005.runbook.v1}` |
| Kampagne löschen | 1 (+ Bestätigen) | 2 (+ Bestätigen) | Blatt „Kampagne löschen …“, Enter → `remove` ×N, Kampagnen 7 → 6 |

Tastatur (Zeilen-„…“, beide Breiten): Fokus auf „…“, Enter öffnet (Fokus auf „Nachrecherchieren“), Pfeil runter →
„Als validiert markieren“, Pfeil hoch zurück, Ende → „Ausschließen“, Esc schließt, Fokus zurück auf „…“;
Leertaste öffnet; Pfeil runter bis Eintrag + Enter führt die Nachrecherche aus (Befehl geschrieben), Menü zu.

Sichtbar nachher (gemessen): offene Firma „Qualifizieren, Weitere Aktionen“ (vorher 6 Knöpfe), ausgeschlossene Firma
nur „…“ (vorher 4), nach Qualifizieren „In Pipeline, …“, nach Pipeline nur „…“; jede Kampagne „Bearbeiten, …“
(vorher 4 Symbole).

Screenshots (selbst angesehen):
- vorher `tests/shots/OUTRES/before/outbound/{1280,420}-01-open.png` (4 Symbole je Kampagne),
  `{1280,420}-02-table.png` (Knopfwand 6/4).
- nachher `tests/shots/OUTRES/after/outbound/`: `-01-open` (✎ + „…“), `-02-table` (eine Hauptaktion + „…“),
  `-03-menu-company-005` (Menü an der Zeile, Ausschließen rot unter Trennlinie, im Fenster gehalten auch bei 420),
  `-04-keyboard-menu`, `-05-after-reject`, `-07-after-qualify`, `-08-after-hide`, `-09-campaign-menu`,
  `-10-import`, `-11-edit`, `-12-delete-sheet`.
- Standard-Rundgang nachher `tests/shots/OUTRES/after-rundgang/outbound/` (rundgang2 `--max-steps 3`): 1280 und
  420 je 6 Schritte, 2 Schreib/Befehle (1 abgelehnt: `knowledge.command` unbekannt im Stub – identisch vorher),
  0 Fehler, 0 Geometrie – gleich wie vorher.
- research vorher/nachher `modules/research/tests/shots/OUTRES/{before,after}/research/`.

## Schritt 4 – Abnahme (08:24–08:26)

| Prüfung | Ergebnis |
|---|---|
| `modules/outbound/tests/v4-api.test.mjs` + `outbound.test.mjs` (Pin 68b83a3) | 37/37 grün (vorher 36 + 1 neu) |
| `modules/outbound/handlers/tests/*.test.mjs` | 163/166, dieselben 2 roten wie vorher (`legacy-forwarders` `ctox:`-Schema, `outbound.automation.sim`), nicht durch OUTRES |
| `modules/research/tests/*.test.mjs` + `test.mjs` | 139/139 grün |
| `node v4/debt/ratchet.mjs` | OK (guard 1838 · scan 3, unverändert) |
| `node scripts/assert-shell-v2-contract.mjs` | 38/38 |
| `node scripts/assert-design-system.mjs` | OK |
| `node v4/tools/guards/textdiaet-rule.mjs --summary` | 33 → 21 (research 16 → 4) |

## Offene Fragen

1. **Baseline des Wächters nicht gesenkt**: `--write-baseline --app modules/research` schreibt
   `v4/tools/guards/textdiaet-baseline.json` (v4/** ist für mich gesperrt). Der Wächter meldet „Unter der Baseline (5)“;
   wer v4 pflegt, sollte research (16 → 4) absenken.
2. **v4-Manifeste veraltet**: `v4/manifests/{outbound,research}.manifest.json` tragen für `index.js` schon vor OUTRES
   andere Hashes (TEXT-E hat sie nicht neu erzeugt); mit OUTRES weichen zusätzlich `index.css`, `locales/*.json`
   ab. Neu erzeugen über `v4/tools/manifest/gen-manifest.mjs` (nicht meine Dateien).
3. **Doppeltes „Ausgeschlossen“** an ausgeschlossenen Firmen: `company-exclude-badge` (`index.js:3625/3683`) plus
   `ctox-badge` in `renderCompanyRowActions`; im Bild ist nur eines sichtbar (eines per CSS verborgen). Nicht
   angefasst – eines davon kann weg.
4. **Ausgeschlossene Firma zurückholen**: Es gab und gibt keine Aktion „wieder qualifizieren“ in der Zeile (die
   Detail-Schublade hat „Ablehnen“/„In Pipeline“). Soll das Menü bei ausgeschlossenen Firmen „Qualifizieren“
   anbieten?
5. **Kampagnen-Symbol**: „Hauptsymbole, die sonst nirgends erreichbar sind“ hätte wörtlich Runbook, Bearbeiten und
   Löschen sichtbar gelassen (nur Import ist auch im Kopf) – das wären wieder 3 Symbole + „…“. Ich habe nur
   „Bearbeiten“ sichtbar gelassen und Runbook (selten, oft ausgegraut) und Löschen (destruktiv, Standard: Menü) ins
   Menü gelegt. Falls der Owner „Löschen“ sichtbar will: ein Eintrag in `renderCampaignItem` zurück.
6. Detail-Schublade der Firma (`index.js:6255-6330`) hat weiter eine eigene Knopfreihe (Validiert, Log, Pipeline,
   Ablehnen) – nicht Teil des Auftrags.
