# KIT2: Inhalts-Palette auf WCAG AA, Automatik-Knopf lesbar, matching bei 420 (Shell v4)

Klon `~/Documents/ctox-shell-v4`, Branch `shell-v4/prototype`, Kopf 2f006b168. Lauf am 16.10.2026, 01:10–02:20.
Kein git außer Lesen, keine Subagents. Alle Zahlen unten habe ich selbst gemessen (Stub `stub-pins/a8765b9`).

## Ergebnis in Zahlen

| Messung | vorher | nachher |
|---|---|---|
| `contrast.mjs --strict` (40 Apps × hell/dunkel, 1280) | 51 von 80 Läufen mit Funden | **0 von 80** (`G10 Kontrast: OK (80 App/Theme-Läufe)`) |
| G11 schmal (420, alle 40 Apps) | 39/40 (matching rot) | **40/40** |
| Apps bei 420 ganz grün (G1–G11) | 25/40 (KIT1) | 26/40 |
| Rahmen-Labor mock / stub | 167/167 / 198/198 (KIT1) | **167/167 / 198/198** |
| G0 Fensterrahmen (stub) | icon/nw/ne/sw/se/cluster = 1 | **= 1** |
| FR6d-G0 Desktop/Crew/Start (stub) | 1/1/1 | **1/1/1** |
| TOP-1 Kopfleiste hell/dunkel 1280/420 | je Drittel 1 | **je Drittel 1** |
| Shell-V2-Vertrag | 38/38 | **38/38** |
| Ratchet | OK | **OK** (guard 1838, 4 Zähler gesunken, nicht festgeschrieben) |

Die Funde vor dem Fix (strikt, Rahmen-Token-Paare):

| Theme | Paar | Kontrast | Häufigkeit |
|---|---|---|---|
| hell | `--muted` #71717b auf `.ctox-well` #f5f5f5 | 4,43 | 138× in 18 Apps |
| hell | `--muted` auf `--accent-soft` #e0e6f7 (gewählte Zeile) | 3,87 | 52× in 20 Apps |
| hell | `--danger` #b45248 auf `--accent-soft` (Badge in gewählter Zeile) | 3,97 | interviews, reports |
| dunkel | `--muted` #818181 auf `.ctox-well` #1d1d1d | 4,33 | 137× in 18 Apps |
| dunkel | `--muted` auf `--accent-soft` #16264f | 3,78 | 52× in 20 Apps |
| dunkel | Kit-Primärknopf #111111 auf `--accent` #346bf1 | 4,06 | 15× in 13 Apps |
| dunkel | `--accent` als Text auf #111/#141414/#191919/`.ctox-well`/`--accent-soft` | 3,17–4,06 | 23× |
| dunkel | #0a0a0a auf `--accent` (conversations-Zähler) | 4,26 | 5× |

## 1. Inhalts-Palette (`v4/frame/theme/build-theme.mjs` → `theme.css`, Block `KIT_CONTENT`)

Die Tokens werden nur im Fensterinhalt gesetzt: `:where(.shell-window-content, .v4-app-root)`.

Am `:root` bleiben die Werte unverändert. Grund: Der Fensterrahmen liest `--accent` (Fokuskante, shell-v2-chrome.css)
und `--muted` (Fensterknöpfe). Kopfleiste, Startmenü, Desktop und Crew-Leiste deklarieren ihre Tokens ohnehin selbst
(`[data-v4-topbar]`, `[data-v4-v2]`). Host-Werte (`--ctox-host-*`) haben weiter Vorrang. Ein Workspace-Branding
(`data-workspace-branding="custom"`) ist ausgenommen.

| Token | hell | dunkel | gemessen (Rechnung, WCAG) |
|---|---|---|---|
| `--muted` | #71717b → **#64646d** | #818181 → **#959595** | hell: auf surface 5,86 · well 5,37 · accent-soft 4,69. dunkel: 6,30 · 5,63 · 4,92 |
| `--accent` | unverändert #1b4ed8 | #346bf1 → **#6690f5** | dunkel als Text: auf surface 6,18 · well 5,52 · accent-soft 4,83 |
| `--accent-foreground` | unverändert #ffffff (6,71) | #ffffff → **`var(--surface)`** #111111 (6,18) | Beschriftung auf dem Akzent |
| `--danger` | oklch(0.56 0.13 28) → **#a4453c** | unverändert #e06b60 | hell: auf accent-soft 4,82 · surface 6,01 |

- **Warum der dunkle Knopf dunkle Schrift trägt:** Ein Akzent, der als Text auf #111 mindestens 4,5 erreicht, braucht
  eine Leuchtdichte ≥ 0,2. Weiß darauf liegt dann unter 4,5 (#6690f5: 3,06). Beides zugleich geht nicht. Dunkel
  beschriftet darum die Flächenfarbe den Akzent, so wie es der Kit-Primärknopf (`color: var(--surface)`) schon tat.
- **Zwischenstand:** Im ersten Wurf galt `--danger #a4453c` auch im dunklen Theme, denn der helle Selektor trifft
  ebenfalls. Folge: 4 Läufe dunkel rot (Badge 2,81). Der helle Block ist jetzt auf `:root:not([data-theme="dark"])` begrenzt.
- `--muted` hell bleibt sichtbar heller als `--text` (#52525b); die Abstufung Text/Meta bleibt erhalten.

## 2. `--accent-foreground` im v4-Fenster (`v4/frame/frame.css`)

Die Ursache des #52525b lag nicht in den Tokens. `--accent-foreground` war im Fenster schon #ffffff (selbst gemessen am
`.v4-app-root`). Die Ursache ist die Rahmen-Rücksetzung `.v4f :where(button) { font: inherit; color: inherit; }`:

- Der Kommentar dazu sagt „Spezifität 0“. Tatsächlich zählt `.v4f` mit, die Regel hat also 0-1-0.
- Sie lädt nach base.css.
- Damit schlägt sie die gleich starke Kit-Regel `.ctox-run-control { color: var(--accent-foreground) }`. Der Knopf erbt
  `--text` (#52525b auf Akzent, 1,15).
- `.ctox-button.is-primary` (0-2-0) war nicht betroffen.

Fix: Die Farb-Rücksetzung nimmt den Automatik-Knopf aus.

```css
.v4f :where(button) { font: inherit; }
.v4f :where(button:not(.ctox-run-control)) { color: inherit; }
```

Die Spezifität bleibt 0-1-0, alle anderen Knöpfe und die Schrift bleiben wie bisher.

Eine allgemeine Lösung habe ich gemessen und verworfen: die Rücksetzung im App-Inhalt ganz auf Spezifität 0 senken.
Dabei änderten sich 16–49 Knöpfe je App, zum Beispiel Pane-Reiter von 13px/400 auf 11px/700 und Sortierknöpfe in der
Farbe. Das wäre ein sichtbarer Umbau aller Apps.

Danach habe ich die app-eigenen Notbehelfe aus DARK entfernt, weil sie überflüssig sind. Ihr Wert (`var(--surface)`)
ist jetzt identisch mit `--accent-foreground` in hell und dunkel.

- `modules/appsec-pentest/index.css`: `[data-app="appsec-pentest"] .ctox-run-control { color: var(--surface); }` samt Kommentar entfernt.
- `modules/shiftflow/index.css`: dieselbe Regel entfernt. Die Avatar-Regel von DARK bleibt; sie betrifft etwas anderes.
- Ergebnis gemessen: Beschriftung „Audit starten“ bzw. „Dienstplan veröffentlichen“ ist hell weiß auf #1b4ed8 (6,71)
  und dunkel #111 auf #6690f5 (6,18). Beide Apps bestehen G10 strikt.

**FR4-4 (gesperrte Kit-Knöpfe ≥ 3:1)** wurde mit der neuen Spreizung im mock-Labor zunächst rot: dunkel 2,74.

- Ursache: Der Test misst einen Frame nach dem Theme-Wechsel. Die Kit-Knöpfe haben `transition: color .15s`, also
  stand noch das helle `--muted` auf dunkler Fläche.
- Fix im Generator (`KIT_DISABLED`): gesperrte Kit-Knöpfe ohne Übergang (`transition: none`). Ein gesperrter Knopf
  hat keinen Hover.
- Danach mock 167/167. Den Test habe ich nicht angefasst.

## 3. matching bei 420 (G11), App-Fall: `modules/matching/index.css`

Die Ursache ist nicht das Kit. Der Titelblock der matching-Pane verdeckt „Zurück“ auch nicht.

- Der Treffer `div.ctox-pane-titles` gehört zu den **Slide-overs** `.jp` und `.object`.
- Geschlossen sind sie nur per `transform: translateX(±100%)` verschoben, bleiben aber sichtbar und treffbar
  (`pointer-events: auto`, `transition: transform .25s`).
- Das Labor misst direkt nach `measureInPage`, dessen G5-Prüfung die Stile neu aufbaut. Die Slide-overs gleiten dann
  gerade hinaus und liegen über dem Fenster. Belegt per Sonde: nach der Messung trifft `elementsFromPoint` am
  Zurück-Mittelpunkt `ASIDE.jp`, Position x=0 bzw. x=−29 bei zwei Läufen, also in Bewegung.
- Das erklärt, warum KIT1s Einzelprobe ohne vorherige Messung grün war.

Fix (Layout, keine Farben): Geschlossene Slide-overs sind sofort nicht mehr treffbar und nach dem Hinausgleiten unsichtbar:

```css
[data-app="matching"] .object:not(.open),
[data-app="matching"] .jp:not(.open) {
  pointer-events: none;
  visibility: hidden;
  transition: transform 0.25s ease, visibility 0s linear 0.25s;
}
```

- Bei reduzierter Bewegung gilt `transition: none`.
- Das Öffnen ist unverändert. `.open` setzt Sichtbarkeit sofort und gleitet herein.
- Nebenwirkung: Geschlossene Slide-overs sind auch nicht mehr per Tab erreichbar. Vorher waren sie es, obwohl sie
  außerhalb des Bildes lagen.
- Den zwischendurch probierten Kit-Eingriff (Detail-Kopf `height: auto`) habe ich zurückgenommen, er war nicht die
  Ursache.

Ergebnis:
- matching G11 grün (Zurück getroffen, führt zur Liste).
- Gesamtlauf 420: G11 **40/40**.
- matching bleibt bei 420 nur G4-rot. Der Titel „Anforderung auswählen“ endet mit einer Ellipse; das war vorher genauso.

## Abnahme (selbst ausgeführt)

| Befehl | Ergebnis |
|---|---|
| `contrast.mjs --strict` (alle Apps, hell+dunkel, 1280) | `G10 Kontrast: OK (80 App/Theme-Läufe) · übersprungen: outbound-lead-generation`; JSON `shots/KIT2/contrast-strict-1280.json` |
| `geometry-v4.mjs --widths 420` (40 Apps) | G11 40/40; ganz grün 26/40 (rot nur G4 bei 14 Apps, G8 bei desktop, wie vor KIT2) |
| `frame.test.mjs mock` | 167/167 |
| `frame.test.mjs stub` | 198/198, darin G0 = 1 (alle 6 Bereiche), FR6d-G0 desktop/crew/start = 1, TOP-1 alle Drittel = 1 |
| `scripts/assert-shell-v2-contract.mjs` | OK 38/38 |
| `v4/debt/ratchet.mjs` | OK |
| `build-theme.mjs --check` | aktuell |
| `gen-manifest.mjs --check` | Exit 0 |
| `node --test contrast.test.mjs` | 3/3 |
| App-Tests appsec-pentest / shiftflow / matching | 11/11 · 9 pass + 1 skip · 14 pass + 1 skip, 0 fail |
| `node --test geometry-v4.test.mjs` | 22/23. Rot ist „Labor-Server liefert nur freigegebene Bibliotheken …“ (`spreadsheets/index.js enthält nach dem Umschreiben kein ctox:lib/ mehr`). Das berührt keine KIT2-Datei, alle G7/G8/G11-Kit-Tests sind grün. Ursache nicht untersucht (parallele Stränge ändern Apps/Manifeste). |

Bilder, selbst angesehen, in `shots/KIT2/`:

- **appsec-pentest hell/dunkel:** „Audit starten“ ist hell weiß auf Blau, dunkel dunkel auf hellerem Blau, gut lesbar.
  Meta in der gewählten Zeile ist lesbar.
- **shiftflow hell/dunkel:** „Dienstplan veröffentlichen“ ist lesbar. Die Kopfleiste ist unverändert.
- **customers Detail hell/dunkel:** Gewählte Zeile mit Meta, Reiter mit „Mehr ▾“. Rahmen, Icon-Header und Hover-Icons
  sind unverändert.
- **matching 420:** Detail mit „‹ Zurück“ rechts vom Icon. Der Treffertest am Mittelpunkt landet auf dem Knopf.

## Geänderte Dateien

- `v4/frame/theme/build-theme.mjs`: `KIT_CONTENT` und `transition: none` in `KIT_DISABLED`.
- `v4/frame/theme/theme.css`: neu erzeugt. Die Kopfzählung der Kit-Nutzer hat sich durch parallele App-Änderungen mit verschoben.
- `v4/frame/frame.css`: Knopf-Rücksetzung.
- `modules/matching/index.css`: geschlossene Slide-overs.
- `modules/appsec-pentest/index.css`, `modules/shiftflow/index.css`: je ein DARK-Notbehelf entfernt.

Nicht angefasst: shared/shell-v2-chrome.css, shared/shell-v2-topbar.css, v4/frame/shell-v2-desktop.css,
v4/frame/pane-narrow.js (kein Kit-Fall) und alle Dateien von FIX9a/b und LOOP-2.

## Offen

- `--muted`/`--accent` am `:root` (Rahmen, Kopfleiste, Desktop) sind unverändert. Wo der Rahmen selbst Text in
  `--muted` zeigt, gilt weiterhin 4,3–4,8. Das misst G10 nicht, weil es den App-Inhalt misst. Eine Änderung dort würde
  G0/TOP/FR6d-G0 berühren und gehört dem Shell-V2-Besitzer.
- Der Kommentar „:where = Spezifität 0“ in frame.css ist ergänzt, nicht umgebaut. Die übrigen Kit-Knöpfe erben
  weiterhin Schrift und Farbe vom Rahmen.
- Die Manifest-Hashes für frame.css/theme.css und die drei App-CSS-Dateien sind nicht neu erzeugt (`--check` war grün).
  Bei der Integration `gen-manifest.mjs` laufen lassen.
- `geometry-v4.test.mjs` hat einen roten Bibliothekstest (siehe oben). Er gehört nicht zu KIT2.
