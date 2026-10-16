# Strang MAN2 – echte Icons aus dem Generator, keine händischen Kantenfarben (15.10.2026)

Owner-Mängel M-09 („apps mit illegalen icon logo“) und M-17 (Kantenfarben „korrekt berechnet und nicht händisch
eingestellt“). Ort: Klon `~/Documents/ctox-shell-v4`, Basis `src/apps/business-os/`.

## 1. Raster-Icons: eine Quelle, Kopie im App-Paket

- Quelle sind die Auswahlprojektionen, die Shell-V2 liest: `shared/operator-icon-selection.js` (35 Apps) vor
  `shared/grok-shell-icon-selection.js` (4 Apps), dann `module.json layout.icon_asset`. Das ist die Reihenfolge von
  `app.js getRegisteredSvgIcon` (Zeile 918f.).
- Welsch-Abgleich am 15.10. 21:2x: `https://welsch.ctox.dev/shared/operator-icon-selection.js` und
  `…/grok-shell-icon-selection.js` sind byte-gleich mit dem Klon (sha256 `d4fb0a7f…` bzw. `1048af9d…`), `app.js` auf
  welsch hat dieselbe Reihenfolge. `modules/kundenpipeline/module.json` und `modules/knowledge/module.json` auf welsch
  nennen dieselben Bilder wie der Klon. OLG ist auf welsch nicht installiert (404). Für OLG gilt deshalb das
  `layout.icon_asset` aus seinem `module.json`; das zeigt auch Shell-V2.
- Generator (`gen-manifest.mjs`): `loadIconSelection` liest beide Tabellen, `packageIconTarget` bestimmt den
  Paketpfad. Liegt das Bild schon im eigenen Paket, gilt dieser Pfad (knowledge: `assets/icon/knowledge-256.png`).
  Sonst gilt die Kopie `assets/icon.<ext>`. `syncPackageIcons` kopiert im Schreiblauf (ohne `--check`) und prüft gegen
  den sha256 der Auswahl. `--check` meldet eine fehlende oder abweichende Kopie als `icon_package` (Exit 1).
  `layout.icon_asset` zeigt danach auf die Paketdatei, `_icon_asset_source` nennt die Herkunft. Fremde
  `shared/…`-Pfade aus `module.json` stehen nur noch in `_icon_asset_replaced`; `srcset`/`_60` auf fremde Bilder
  entfallen.
- module.json bleibt unverändert, Shell-V2 ist also unberührt.
- 38 Bilder kopiert: `modules/<app>/assets/icon.jpg` (34) bzw. `assets/icon.png` (creator, desktop, explorer,
  file-viewer).
- Ergebnis: 41 von 41 Apps tragen ein echtes Icon (`stats.icon_raster` 40, `icon_svg` 1 = OLG,
  `icon_generated` = []). Vorher hatten 9 Apps weder `icon_asset` noch `icon_svg` und zeigten im v4-Launcher das
  generierte Buchstaben-Icon: appsec-pentest, consent, esign, intake, interviews, nachweise, placements,
  submissions, support.
- **Apps ohne Bild (Ersatz-Icon FR6a): keine.**
- Tests: `MAN2 heutiger Baum…` (jede App mit Auswahl trägt die Paketkopie mit Auswahl-sha256, kein frame_palette)
  und `MAN2 Mini-Repo…` (Prüflauf kopiert nicht und meldet; Schreiblauf kopiert; Quelle fehlt → Verstoß; ohne Bild
  → `icon_generated`).

## 2. frame_palette

- Der Generator übernimmt `layout.frame_palette` nicht mehr; in keinem Manifest steht das Feld noch.
- Neue Wächterregel `v4/tools/guards/frame-palette-rule.mjs` mit Test `frame-palette-rule.test.mjs`. Sie meldet jeden
  Schlüssel `frame_palette` in `<app>/module.json` und `<app>/v4-manifest.json` unter modules/, customer-modules/ und
  desktop-apps/, in jeder Tiefe; kaputtes JSON zählt das Textvorkommen.
  Ratchet über `frame-palette-baseline.json`:
  - `--check`: Exit 1 über der Baseline.
  - `--write-baseline`: senkt nur; neue Treffer nur mit `--force`.
- Altbestand (Baseline, 2): `modules/knowledge/module.json`, `customer-modules/outbound-lead-generation/module.json`.
  Beide bleiben vorerst unverändert. Shell-V2 liest das Feld dort womöglich noch. Für OLG gilt außerdem die
  Owner-Auflage, das Manifest nicht anzufassen.
- Eigene Datei statt einer Regel in `guard-v4.mjs`: eine neue guard-v4-Regel hätte den Ratchet in
  `v4/debt/ratchet-baseline.json` steigen lassen. Diese Datei bearbeitet gerade ein anderer Strang.
- Nicht angefasst: `v4/shell/kernel-adapter.js` führt `frame_palette` noch in `LAYOUT_KEYS`. Das ist wirkungslos, weil
  kein Manifest das Feld mehr trägt; die Datei liegt außerhalb dieses Strangs.

## 3. foreign_collections aus fb2db7a51

Neu erzeugt: placements, submissions, interviews, nachweise und consent lesen `customer_accounts` (Eigentümer
customers). intake liest `customer_accounts`. `applications` (intake) war schon deklariert. Die exports tragen das:
`intake.exports.applications` = read, write; `customers.exports.customer_accounts` = read. `foreign_without_export`
ist 0.

## 4. Shell-Manifest: Meldeschleife

- Die Shell-Deklaration steht jetzt als Konstante `SHELL_FOREIGN` in `gen-manifest.mjs` und wird in
  `buildShellManifest` verwendet. Damit liefern der volle Lauf und `--shell-libs` dieselben Einträge.
- Neu: `business_module_reports` und `ctox_bug_reports` mit read und write, Eigentümer ctox. Beleg:
  `v4/shell/report.js REPORT_COLLECTIONS`.
- `generate()` prüft, dass der Eigentümer laut Zuordnung stimmt, sonst gibt es den Verstoß `shell_foreign_owner`.
  Die ops landen in den exports der Eigentümer-App: `ctox.exports.ctox_bug_reports` war nur read, jetzt
  read und write.
- In Mini-Repos ohne diese Collections entfallen die Einträge mit `_todo`.
- Test: `MAN2 Shell-Manifest…`.

## 5. Warum out/ und REPORT-M2.md schon vorher abwichen

`--check` meldete vor MAN2 alle 42 Manifeste, out/* und REPORT-M2.md als abweichend. Neu erzeugt ist alles wieder
gleich. Ursachen, belegt durch einen Vergleich der Ausgaben vorher und nachher:

1. Die `files`-Hashes änderten sich durch Commits ohne Neuerzeugung: TEXT-B/TEXT-C Runde 2 (2995e300b, 5a4af5101)
   änderten index.js, index.html und index.css vieler Apps. fb2db7a51 änderte die Deklarationen, ohne neu zu erzeugen.
2. Das Plattformmanifest `~/.local/state/ctox-sync-v4/docs/v4-manifests/ctox-platform.manifest.json` liegt außerhalb
   des Repos und änderte sich um 21:24. Neu sind die Plattform-Collections `coding_turns` und `devices`
   (`owned_by_platform` 13 → 15, `collections_total` 231 → 233).
3. OLG4 (02773df88) entfernte `browser.capture`; der Test „heutiger Baum – Wünsche…“ erwartete das noch und war vor
   MAN2 rot. Jetzt prüft er das Gegenteil. `capabilities_distinct` 77 → 74.
4. Die Shell-Dateien änderten sich durch die parallelen Stränge FR6b, TOPBAR und ZONE45 (`shell_files` 69 → 74).

Folge: `--check` ist nur für einen Stand grün. Jede Änderung an App-Dateien, an v4/frame, v4/shell oder am externen
Plattformmanifest macht ihn wieder rot, bis neu erzeugt wird. Während der MAN2-Arbeit geschah das zweimal durch
FR6b/ZONE45 (v4/frame/index.js, v4/shell/status-zone.js, v4/shell/sync-state.js).

Der zweite rote Test vor MAN2 war „MAN: Trigger aus Code-Export…“. `scanTriggerExports` wertete Trigger-Literale mit
`vm` und einem Zeitlimit von 50 ms aus. Bei Last 32 lief dieses Limit ab, und die Code-Trigger fielen still weg.
Das Limit liegt jetzt bei 2000 ms; es dient nur als Schutz gegen Endlosschleifen in einem reinen Literal.

## 6. Abnahme (Zahlen vorher → nachher)

| Prüfung | vorher | nachher |
|---|---|---|
| `gen-manifest.mjs --check` | Exit 1 (47 Dateien abweichend) | Exit 0 nach Neuerzeugung (Stand 21:3x) |
| `node --test v4/tools/manifest/*.test.mjs` | 35/37 (2 rot, s. o.) | 40/40 |
| `node --test v4/tools/guards/frame-palette-rule.test.mjs` | – | 4/4; `frame-palette-rule.mjs --check` Exit 0 (2 = Baseline) |
| `check-caps.mjs` (Manifest-Modus) | OK, 75 OS-Primitive, 2 in Baseline | OK, 72 OS-Primitive, 2 in Baseline (weniger durch OLG4/Neuerzeugung) |
| `check-caps.mjs --calls` | 204 · ABWEICHUNG 0 · HUELLE 0 · UNGEPRUEFT 2 · OHNE_VERTRAG 22 | unverändert |
| `check-isolation.mjs` | 151 (R1 1, R3 2, R4 148) | 152: +1 R3 `v4/frame/topbar.js:148 App-ID ctox` (Strang TOPBAR, nicht MAN2) |
| Rahmen-Labor mock / stub (Pin 68b83a3) | – (Labor liest keine Manifeste und keine Module) | 125/130 bzw. 139/144 |

Die 5 roten Fälle sind in beiden Läufen dieselben: Shell-Tausch (ein Satz Shell-CSS), Doppelpuffer (2×),
Theme-Schnittstelle und G0 Shell-Treue (Ecken sw/se). Die Linkliste enthält neu `shared/shell-v2-topbar.css`
(TOPBAR). Dazu kommen nicht committete Änderungen anderer Stränge an `frame.css` und `v4/frame/index.js`. Das Labor
lädt nur `lab/apps`, v4/frame und shared/; MAN2 ändert davon nichts, also kein neuer Fall durch MAN2. HEAD-Stand der
Ergebnisdateien: mock 79/79, stub 97/97 (ältere Fallzahl).

Launcher bei 1280, selbst angesehen: Kern-Stub 68b83a3, echter v4-Rahmen, alle 41 Apps mit `layout` aus
v4/manifests. Ad-hoc-Skript im Scratchpad, weil das Geometrie-Labor je Seite nur eine App rendert. 41 von 41
Kacheln haben `data-v4-icon-source="icon"` und keine Seitenfehler; kein Buchstaben-Icon mehr. Hinweis: Die Kacheln
laden 1024-px-Bilder (JPEG ~100–150 KB) für 40 px. Ein Screenshot ohne vorheriges `img.decode()` zeigt kurz leere
Kacheln. Das ist ein Kandidat für eine 256-px-Kopie in einem Folgestrang, kein Fehler im Manifest.
