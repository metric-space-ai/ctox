# DARK – dunkel auf dunkel im hellen Theme, Kontrast-Kriterium G10, letzte Roh-IDs (Shell v4)

Klon `~/Documents/ctox-shell-v4`, Basis `src/apps/business-os/`, Kopf 7750b5a37, Stand 16.10.2026 00:35–01:30.
Kein git außer Lesen. Nicht angefasst: v4/frame, shared/, Shell-CSS (app.css), module.json/collections.schema.json
von outbound-lead-generation, alle Handler.

## 1. Feste dunkle Flächen → Theme-Tokens (12 Apps + knowledge)

Ursache wie in FIX8: `index.css` setzte Modul, Pane und Kopfzeilen auf `var(--shell-v2-surface(-alt), …)`. Im
Shell-v2-Fenster (`.shell-window[data-shell-contract="v2"]`, app.css) sind das feste dunkle Werte (#10151c/#141b24),
auch im hellen Theme. Text und Symbole nutzen die hellen Theme-Tokens. Ergebnis: Kopfzeilen 1,16, Symbole 2,24.

Fix (Muster esign/calendar/knowledge): Flächen über die Theme-Tokens.

| vorher | nachher |
|---|---|
| `var(--shell-v2-surface, var(--bg))` | `var(--bg)` |
| `var(--shell-v2-surface, var(--surface))` | `var(--surface)` |
| `var(--shell-v2-surface-alt, var(--surface))` | `var(--surface)` |
| `var(--shell-v2-surface-alt, var(--surface-2))` | `var(--surface-2)` |

- Apps: app-store, buchhaltung, consent, conversations, customers, importer, intake, interviews, invoices, iot, mail, news.
- knowledge: Schublade `var(--ctox-surface, #10151c)` → `var(--surface)`.
- Danach: `grep -E -- '--shell-v2-surface|#10151c|#141b24|#0b0f14' modules/*/index.css customer-modules/*/*.css` findet nur noch Kommentare (FIX8/v4-Hinweise).

## 2. Kontrast-Kriterium „G10 Kontrast“

Neu: `v4/tools/geometry/contrast.mjs` (+ `contrast.test.mjs`).

- Startet jede App im echten v4-Rahmen über ihr Desktop-Symbol (`[data-v4-desk-app]`, echter Klick wie rundgang2 und
  check-raw-ids). Server und Fixtures kommen aus `rundgang.mjs`.
- Maximiert bei 1280, je Theme `light`/`dark` (colorScheme plus `data-theme`).
- Misst im geöffneten Zustand und nach Klick auf den ersten Datensatz.
- Gemessen wird jeder sichtbare Text und jedes einfarbige SVG-Symbol bis 48px gegen den tatsächlichen Hintergrund.
  - Farbschichten werden gemischt, ebenso Verläufe (die Shell-v2-Fläche ist ein `linear-gradient`).
  - Die Deckkraft der Vorfahren wird eingerechnet.
  - Farben werden über ein Canvas normalisiert (color-mix inklusive).
- Bereich je Fund: Kopf, Liste, Detail oder Inhalt.
- Schwellen nach WCAG: Fließtext 4,5, große Schrift (≥ 24px oder ≥ 18,66px fett) 3, Symbole 3.
- Nicht gemessen: deaktivierte Elemente sowie mehrfarbige SVGs (Porträts, Diagramme sind Illustrationen).
- **Rahmen-Token**: Ein Fund zählt nicht als App-Fund, wenn Vorder- UND Hintergrund exakt Theme-Tokens des Rahmens
  sind (oder die Kit-Mulde `.ctox-well`) und der Kontrast ≥ 3 liegt. Diese Funde werden als `TOKEN`-Zeilen und im
  JSON-Feld `.token` gemeldet. `--strict` zählt sie mit.
  - Unter 3 ist ein Token-Paar immer ein App-Fehler. So wurden appsec-pentest und shiftflow gefunden.
- Exit 1 bei App-Funden. Apps ohne Laborkonfiguration werden wie bei check-raw-ids übersprungen (SKIP).

```
node v4/tools/geometry/contrast.mjs [--app a,b] [--width 1280] [--themes light,dark] [--json f] [--shots dir] [--strict]
```

**Mutationsbeleg:** customers/index.css auf den Vorher-Stand gesetzt → `LOW customers light … Kopf h2.ctox-pane-title "Kunden" 1.16<4.5 #27272a/#141b24 · Kopf svg "[Symbol] Kunde anlegen" 2.24<3`. Danach zurückgestellt (diff leer) → `G10 Kontrast: OK`.

### Messung vorher → nachher (alle 40 Apps × 2 Themes, 1280)

| | vorher | nachher |
|---|---|---|
| Läufe mit App-Funden | 61 von 80 (ohne Token-Trennung) bzw. 43 (mit) | **0 von 80** |
| geprüfte Knoten | – | 4850 |
| dunkel auf dunkel (#141b24/#10151c als Hintergrund) | 12 Apps, bis 1,05 | 0 |

### App-eigene Kontrastfehler, behoben

| App | Befund vorher | Fix |
|---|---|---|
| consent, intake | Metazeilen `--muted` 80 % → 3,1/3,3 | volle `--muted` |
| cv-print-builder | Kartenmeta `opacity .82` → 3,0–3,6 | Abschwächung entfernt |
| knowledge | Domäne `opacity .85` 3,6. Meta auf gewählter Karte 4,0. Aktionssymbole `opacity .55` 2,6 | ohne Abschwächung. Meta auf gewählter Karte zum Text gemischt. Symbole `.8` |
| browser | Zustandssymbole mit Rückfallfarben `--ctox-warn/-accent/-text-muted`, die es im v4-Rahmen nicht gibt (1,5–2,4). Detailzeile `opacity .85` 3,0 | Symbole über `--muted` bzw. zum Text gemischt; Abschwächung entfernt |
| news | Palette `--news-muted` 64 % (2,4–3,4). Kürzel hell auf Tonfarbe 55 % (dunkel 2,6) | 80 %. Kürzel/Avatar: Tonfarbe 30 % als Schleier, Zähler im Reiter in Reiterfarbe |
| conversations, outbound, shiftflow | `--muted` auf eigenen Schleiern 4,1–4,4. Sortiersymbol `--muted` 70 % 2,7 | 30 % zum Text gemischt; Sortiersymbol volle `--muted` |
| ctox | Crew-Name `--crew-color` 72 % → hell 3,7 | 50 % |
| explorer, shiftflow | reiner Akzent auf Akzentschleier, dunkel 3,3–3,7 | Akzent 60 % zum Text |
| importer | Ordnersymbol in `--app-accent-foreground`, dunkel 1,4 | Akzent 45 % zum Text |
| buchhaltung | Inline-`opacity:0.75–0.85` an 6 Stellen in index.js (4,4 auf gewählter Zeile) | `color: var(--muted)` |
| appsec-pentest, shiftflow | **Beschriftung des Automatik-Knopfs unsichtbar**: `.ctox-run-control` nimmt `--accent-foreground` = #52525b auf #1b4ed8 (1,15) | App-seitig `color: var(--surface)` wie der Kit-Primärknopf |
| shiftflow | Avatar-Initialen weiß auf heller Fläche (1,25) bzw. auf Grün (1,5) | Mitarbeiterfarbe als `--emp-color` (index.js, 6 Stellen), Fläche 30 % Schleier, Initialen `--text` |

## 3. Roh-IDs

- **reports**: Das Feld Modul zeigte „module-001…003“, die Liste dazu „standard/email/internal“. `moduleLabel` lieferte
  ohne Katalogeintrag die Roh-ID.
  - Jetzt nur der App-Titel aus `business_apps` (`title`/`name`), sonst nichts.
  - Das Feld Modul entfällt dann ganz, die Listenzeile zeigt nur das Datum (kein loser Trenner).
  - Der Delegationsprompt an CTOX (Zeile 629) behält die ID, weil er maschinenseitig ist.
- **tickets**: Der Ticketschlüssel fiel auf die Datensatz-ID zurück (`ticket_key || id` → „item-001“). Jetzt nur noch `ticket_key`.
- `check-raw-ids` 1280 und 420: **0 für alle 40 Apps**. outbound-lead-generation bleibt SKIP (keine Laborkonfiguration, wie bisher).

## Abnahme (selbst gefahren, `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/a8765b9`)

| Prüfung | Ergebnis |
|---|---|
| App-Tests `node --import ./v4/tools/node/ctox-shared.mjs --test modules/<app>/tests/*.test.mjs` | 0 fail: app-store 15, appsec-pentest 10, browser 11, buchhaltung 21, consent 24, conversations 12, ctox 24, customers 16, cv-print-builder 11, explorer 11, importer 8, intake 25, interviews 22, invoices 93, iot 11, knowledge 15, mail 41, news 19, outbound 12, reports 10, shiftflow 9, tickets 12 |
| `node --test v4/tools/geometry/contrast.test.mjs` | 3/3 |
| `contrast.mjs` (alle Apps, hell+dunkel) | `G10 Kontrast: OK (80 App/Theme-Läufe) · übersprungen: outbound-lead-generation`, Exit 0 |
| `check-raw-ids.mjs` 1280 / `--width 420` | 0 Treffer, Exit 0 |
| `scripts/assert-shell-v2-contract.mjs` | OK: 38/38 Apps |
| `v4/debt/ratchet.mjs` | **FEHLER, aber nicht aus DARK**. Gestiegen ist nur `guard.app.v4.vocab:de-nicht-verfuegbar` 1 → 2, Treffer `v4/source/source-mode.js:872`. Die Datei ist uncommittet von einer parallelen Sitzung geändert, ich habe sie nicht angefasst. Alle App-Zähler gleich oder niedriger. |

Belege: `shots/DARK/vorher-nachher-hell-1…6.png` (je App links vorher, rechts nachher, hell, 1280) und
`shots/DARK/contrast-1280.json` (Endlauf).

- Selbst angesehen: alle 12 Flächen-Apps sowie knowledge, browser, cv-print-builder, ctox, explorer, outbound,
  shiftflow, reports, tickets und appsec-pentest.
- Vorher: dunkle Fenster mit kaum lesbaren Kopfzeilen. Nachher: helle Arbeitsflächen, Kopfzeilen lesbar.
- shiftflow: „Dienstplan veröffentlichen“ und die Initialen sind jetzt sichtbar.
- reports/tickets: ohne Roh-IDs.

## Offen (nicht DARK, nur vermerkt)

- **Rahmen-/Kit-Palette (Besitzer FR6d/Shell-v2, nicht App):** Die Token-Paare liegen knapp unter WCAG, in fast allen Apps.
  - `--muted` auf `.ctox-well`: hell 4,43, dunkel 4,33 (je ~138×, 18 Apps).
  - `--muted` auf `--accent-soft` (gewählte Zeile): 3,87 / 3,78 (52×, 20 Apps).
  - dunkles Theme, Akzent #346bf1 als Text auf #111111: 4,06. Kit-Primärknopf #111111 auf #346bf1: 4,06.
  - Fix gehört in theme.css/shell-v2-desktop.css/app.css (`--muted` hell etwa #6b6b74, dunkel etwa #8a8a8a; Akzent dunkel heller) oder in `.ctox-well`.
  - `contrast.mjs --strict` zeigt sie als Fehler.
- **`--accent-foreground` im v4-Fenster** ergibt hell #52525b statt Weiß. Das trifft jede `.ctox-run-control` (Kit).
  appsec-pentest und shiftflow sind app-seitig umgangen; die Ursache liegt im Rahmen bzw. im Kit.
- **outbound-lead-generation** wird weder von check-raw-ids noch von contrast.mjs gemessen (keine Laborkonfiguration für customer-modules). Ihre CSS hat keine Shell-v2-Flächen (grep).
- **Roh-Werte, die check-raw-ids nicht erkennt** (ohne Ziffern):
  - appsec-pentest-Liste: „projektplan“, „lieferantenbewertung“ (Slug-Kleinschrift).
  - tickets: Quelle „ctox“.
- **interviews**: Datum „01.01.70, 01:00“ in der Liste (Epoch 0), wie FIX8 Nr. 6 bei knowledge.
- **Parallel geändert, nicht von mir** (uncommittet im Klon):
  - v4/frame/*, v4/source/*, v4/shell/*, geometry-v4.mjs (G11/KIT1), g11.mjs.
  - Manifest-Hashes in v4/manifests/<app>.manifest.json, mitgezogen für meine Dateien.
  - Dadurch zeigen die Nachher-Bilder z. B. „Mehr ▾“ statt überlaufender Reiter. Das hat DARK nicht verursacht.
- `modules/*/tests/shots/*.png` haben die App-Tests beim Lauf neu geschrieben.
