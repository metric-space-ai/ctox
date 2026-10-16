# Ladeweg-Messung Shell v4 – Service Worker vs. Blob-URLs

Strang P1 (Shell-v4-Entwurf §4/§5, Sync-v4 §9.1, §10.1, §10.2a, §16.15). Stand Do 15.10.2026, Klon
`~/Documents/ctox-shell-v4` @ 4794af03d, Zweig `shell-v4/prototype`. Nur Gemessenes; Ungemessenes steht als „offen“ da.

## Kurzfassung

**Empfehlung: Service Worker bleibt der Regelweg, Blob-URLs nur als Rückfall – mit zwei Pflichtergänzungen am
Vertrag** (Shell-Bindung in der URL, Claim nach hartem Neuladen; siehe „Entscheidung“).

- Beide Wege bestehen auf Chromium 148 und WebKit 26.4 (Playwright, headless, macOS) die Szenarien a–f und h.
  Der Server bekam in allen 4 Läufen × 4 Kombinationen **0 Anfragen unter `/~a/`** (kein Netz-Rückgriff).
- Der Ausschlag gegen Blob kommt nicht aus der Zeit, sondern aus Szenario g: das heutige App-Muster
  `loadModuleMessages(import.meta.url, …)` und `new URL(\`./locales/${lang}.json\`, import.meta.url)` **scheitert im
  Blob-Weg in beiden Browsern** (`Invalid URL`), im SW-Weg läuft es unverändert.
- Zeit: beide Wege liegen weit unter dem Ziel „Aktivierung p95 ≤ 2 s“. Live-Remount einer frischen Version (neue
  URLs, 20×): Chromium p95 5,8–8,4 ms (SW) / 4,5–7,6 ms (Blob); WebKit p95 41–75 ms (SW) / 10–102 ms (Blob).
  Blob ist auf Chromium 1–2 ms schneller; auf WebKit streuen beide stark (Maschine unter Fremdlast, Load 18–45).

## Aufbau

| Datei | Rolle |
|---|---|
| `v4/sw.js` | Service Worker nach Vertrag: `/~a/<app_id>@<app_ver>/<pfad>` nur aus IDB (`manifests`, `artifacts`), sha256 je Auslieferung, 404 sonst, `ctox:shared/x` → `/~a/shell@<active:shell>/shared/x`, Content-Type je Endung, `immutable`. Zusätzlich (gemessen begründet): optionale Shell-Bindung `~shell@<v>` in der URL und `message 'ctox:claim'`. |
| `v4/blob-loader.js` | Gleiche Funktion ohne SW: Blob-URL je Datei, rekursives Umschreiben von Importen, `new URL('lit', import.meta.url)` und CSS-`url()`; `resolver()` für berechnete Pfade. |
| `lab/ladeweg/kernel-attrappe.js` | IDB `ctox-v4-lab` (Stores `artifacts`, `manifests`, `meta`), `installVersion`, `setActive`, `tamperArtifact`. |
| `lab/ladeweg/lab.js`, `index.html` | Laborseite + Loader-/App-Host-Attrappe (`?mode=sw|blob`), Live-Remount nach §5 (snapshot → neu montieren → alt unmount). |
| `lab/ladeweg/server.mjs` | 127.0.0.1, Zufallsport, Positivliste `/`, `/lab.js`, `/kernel-attrappe.js`, `/blob-loader.js`, `/sw.js`; `/~a/*` → 410 und Zähler. |
| `lab/ladeweg/fixtures/` | `demo` v1/v2 (relativer Import, `fetch` index.html + locales/de.json, `<link>` index.css mit `url(./icon.svg)`, `<img>` icon.svg, `ctox:shared/greet.js`), `shell` v1/v2 (`shared/greet.js` → `./tag.js`, `shared/i18n.js`), `demo-dyn` (berechnete Pfade). |
| `lab/ladeweg/measure.mjs` | Playwright-Messung, schreibt `results.json` + Block unten nach jeder Szenario-Messung. |
| `lab/ladeweg/probe-hardreload.mjs` | Zusatzprobe harter Neuladen (Chromium, CDP). |
| `results-lauf1..4.json`, `results-hardreload.json` | Rohdaten. |

App-Code kommt nie über HTTP: das Messskript reicht die Dateien als Daten in die Seite (Ersatz für CTOX Sync Spur 4),
die Kernel-Attrappe schreibt sie inhaltsadressiert nach IDB.

Aufruf: `node measure.mjs chromium,webkit sw,blob` (≈ 30 s), `node probe-hardreload.mjs`.

## Messwerte über 4 vollständige Läufe (Spannweite min–max, ms)

Jeder Lauf: je (Browser, Weg) eigener Server-Port = eigener Origin (frischer SW, leere IDB). Maschine während aller
Läufe unter Fremdlast (load average 18–45 auf dem Mac); WebKit liefert `performance.now()` nur ganzzahlig.

| Messgröße | chromium/sw | chromium/blob | webkit/sw | webkit/blob |
|---|---|---|---|---|
| a Loader-Init (SW registrieren bis Kontrolle / Blob-Loader anlegen) | 9.3–35.2 | 1.6–3.2 | 10–86 | 2–27 |
| a Erstmontage demo v1 (import + fetch html/json + css + img) | 8.8–28.4 | 6.9–10.2 | 30–137 | 16–75 |
| b Live-Remount frische Version, p50 (20×) | 5.1–7 | 4–6.1 | 12–33 | 6–23 |
| b Live-Remount frische Version, p95 (20×) | 5.8–8.4 | 4.5–7.6 | 41–75 | 10–102 |
| b Remount bereits geladener Version (v1↔v2), p50 (20×) | 1.2–2.2 | 0.6–1.6 | 3–7 | 1–6 |
| h Last kalt: App 1010 KB/41 Module + Shell 1503 KB/30 Module | 38.2–56.9 | 41.6–54.3 | 69–195 | 94–227 |
| h Last: frische App-Version, Shell warm, p50 (5×) | 22.7–39.5 | 22.2–30.1 | 27–67 | 55–76 |
| f Tab 2 folgt Aktivierung (BroadcastChannel → Remount), p50 (10×) | 5.9–8.5 | 5.2–6.2 | 10–31 | 6–24 |
| f Tab 2 folgt Aktivierung, p95 (10×) | 6.5–17.6 | 5.8–7.4 | 20–95 | 14–66 |
| IDB-Installation einer demo-Version (6 Dateien), p50 | 0.5–0.7 | 0.4–0.6 | 1–3 | 1–3 |
| Server-Treffer unter `/~a/` | 0 | 0 | 0 | 0 |

„Live-Remount“ = `snapshot()` alt → `import()` neu → `mount(root, api, {restored})` → `unmount()` alt; Download aus
dem Netz ist nicht enthalten (Artefakte liegen schon in IDB, Zustand `fetched`).

## Szenario-Befunde

- **a Erstinstallation**: beide Wege, beide Browser ok – relativer Import, `fetch(new URL('./index.html', import.meta.url))`,
  `locales/de.json`, `<link>` auf index.css inkl. `url(./icon.svg)` im CSS, `<img>` icon.svg, `ctox:shared/greet.js`
  (das selbst `./tag.js` relativ importiert).
- **b Aktivierung v2**: v2 läuft, halb ausgefüllte Eingabe kommt über `snapshot`/`restored` an, das alte v1-Modul läuft
  unberührt weiter (Modulzustand `mountCount` und `greet` unverändert). **Befund CSS**: das noch offene v1-Fenster
  zeigt die v2-Rahmenfarbe (`rgb(0, 0, 200)` statt `rgb(200, 0, 0)`), in allen 4 Kombinationen. Scoping nur unter
  `[data-app="<id>"]` (Entwurf §6) trennt zwei gleichzeitig montierte Versionen derselben App nicht. Während des
  §5-Wechsels („neues CSS zuerst, dann altes entfernen“) gilt also kurz das neue CSS auch für das alte Fenster; bleibt ein
  Fenster auf der alten Version (Rückfall, Fehlschlag eines Fensters), ist es dauerhaft falsch gestylt.
  → Vorschlag: Shell setzt `data-app-ver` am Inhaltsbereich und der Port-Wächter verlangt `[data-app="<id>"]`; die Shell
  remountet alle Fenster einer App immer gemeinsam (dann reicht es), oder App-CSS wird beim Ausliefern zusätzlich auf
  `[data-app-ver="<ver>"]` gescoped. Nicht umgesetzt.
- **c Shell-Wechsel**: neue Mounts einer **neuen** App-Version und jedes neue Dokument bekommen das neue `greet`; offene
  Fenster behalten das alte. **Befund SW (beide Browser)**: ein neuer Mount derselben App-Version im selben Dokument
  (`demo@v1`, schon geladen) bekommt das **alte** `greet (shell v1)`. Ursache: die Modul-Map des Dokuments hält
  `/~a/demo@v1/index.js` samt umgeschriebenem Import auf `shell@v1`; der SW wird gar nicht mehr gefragt. Die Antwort des SW
  hängt außerdem von `active:shell` ab, ist also entgegen `Cache-Control: immutable` nicht unveränderlich.
  Der gemessene Vorschlag `/~a/demo@v1~shell@v2/index.js` (Shell-Version in der URL, vom App-Host gesetzt) liefert in
  beiden Browsern `(shell v2)` und macht jede URL wirklich unveränderlich. Der Blob-Loader schlüsselt ohnehin je
  Shell-Version und bekommt das neue `greet`.
- **d Server gestoppt**: Wechsel v1/v2 und eine nach dem Stopp installierte neue Version laufen in allen Kombinationen;
  Chromium: SW per CDP `ServiceWorker.stopAllWorkers` beendet, nächste Montage startet ihn ohne Netz neu (ok, Remount 6,8–18,3 ms über 4 Läufe).
  Für WebKit gibt es diesen Eingriff in Playwright nicht (offen).
- **e Manipulierte Datei**: Inhalt hinter dem Hash von `util.js` überschrieben → SW antwortet 404, `import()` scheitert,
  `globalThis.__pwned` bleibt unberührt; Blob-Loader wirft `Hash falsch`. Unbekannte Version / unbekannte Datei → 404.
  **Befund Inhaltsadressierung** (erster Lauf): `demo@bad` teilte den Hash von `util.js` mit `demo@v1`; die Manipulation
  traf damit auch v1, der Blob-Weg verweigerte v1 nach dem nächsten Shell-Wechsel korrekt. Folge für den Kernel: ein
  Hash-Fehler betrifft jede Version, die den Hash referenziert → Artefakt neu holen (Spur 4), nicht nur die eine Version
  als defekt markieren. Messskript seither mit eigenem Inhalt für `bad`.
- **f Zwei Tabs**: Tab 1 installiert und aktiviert, Tab 2 folgt per BroadcastChannel; beide zeigen dieselbe Version und
  dasselbe `greet`; gleichzeitiges Montieren derselben neuen Version in beiden Tabs ok (ein SW bedient beide).
- **g Berechnete Pfade** (Zusatz, heutiges Muster): SW ok in beiden Browsern; **Blob scheitert in beiden**
  (`new URL('./locales/de.json', 'blob:…')` ist ungültig). Der Blob-Weg trägt die heutigen Apps also nicht ohne Umbau.
- **h Last**: 2,5 MB in 72 Modulen, alle per sha256 geprüft und (SW) umgeschrieben: erster kalter Mount 38–57 ms
  (Chromium), 69–227 ms (WebKit). Prüfen+Umschreiben im SW kostet auf Chromium messbar nichts gegenüber Blob.
- **Harter Neuladen** (Zusatz, nur Chromium, `results-hardreload.json`): nach `Page.reload({ignoreCache:true})` ist die
  Seite **unkontrolliert**, obwohl der SW aktiv ist; `/~a/…` geht ans Netz (Server: 410, 1 Treffer) und die Kontrolle
  kehrt in 3 s nicht von selbst zurück. Ein iframe im selben Dokument ist kontrolliert. Gegenmittel gemessen: Loader
  schickt `'ctox:claim'`, der SW ruft `clients.claim()` → kontrolliert nach 0,7 ms, `/~a/` wieder 200, Montage ok.
  Deshalb trägt `sw.js` diesen einen Lebenszyklus-Handler, und die Positivliste muss `/~a/*` mit 404/410 beantworten,
  nie mit Inhalt. WebKit/Safari: ungemessen.

## Entscheidung SW vs. Blob

**Service Worker als Regelweg bestätigt.** Begründung aus den Messungen:

1. Der SW-Weg trägt die heutigen App-Muster unverändert (Szenario g, plus die Zählung unten: `import.meta.url` 178× in
   40 Apps, davon ~96 nicht in der statisch umschreibbaren Form). Der Blob-Weg scheitert daran in beiden Browsern.
2. Zeitlich kein relevanter Unterschied: beide Wege 2–3 Größenordnungen unter dem 2-s-Ziel; Blob ist auf Chromium
   ~1–2 ms schneller (kein SW-Umweg), auf WebKit überlappen die Spannweiten.
3. Sicherheit gleichwertig: beide prüfen sha256 vor der Ausführung (e); der SW zusätzlich für jedes Asset, das der
   Browser selbst lädt (CSS `url()`, `<img>`), ohne App-Mitwirkung.

Pflichtergänzungen, sonst ist der SW-Weg nicht korrekt:

- **Shell-Bindung in der URL** (Befund c): App-Host importiert `/~a/<app>@<ver>~shell@<shell_ver>/index.js`
  (in `sw.js` umgesetzt, Vertragsform ohne Segment bleibt gültig). Ohne das bekommen neue Fenster einer schon
  geladenen App-Version nach einem Shell-Update die alte `shared/`-Fassung. Vertragsänderung an §16.15 nötig
  (Sitzung „ctox sync v4“), sonst bleibt nur: Shell-Wechsel remountet alle Apps mit neuer App-URL.
- **Claim nach hartem Neuladen**: Loader prüft `navigator.serviceWorker.controller`; fehlt er bei aktiver
  Registrierung → `postMessage('ctox:claim')`, erst danach Shell laden. Ohne aktive Registrierung (erster Start,
  SW nicht verfügbar) → Blob-Weg.
- **Positivliste**: Der Host beantwortet `/~a/*` mit 404/410 (gemessen: sonst landet ein unkontrollierter Abruf im Netz).

Blob-Rückfall bleibt sinnvoll für Plattformen ohne SW, aber nur für Apps, die beim Port die unten genannten Umbauten
für den Blob-Weg haben. Empfehlung: Port-Regel „berechnete Pfade nur über `api.shell.assetUrl(path)`“ für alle Apps,
dann trägt der Blob-Weg ebenfalls (im SW-Weg liefert `assetUrl` einfach die versionierte URL).

## Plattformrisiken

| Plattform | Stand |
|---|---|
| Chromium 148 (headless shell) | gemessen, alle Szenarien |
| WebKit 26.4 (Playwright-Build, macOS, headless) | gemessen, alle Szenarien außer SW-Neustart (d) und harter Neuladen. Das ist **nicht Safari**: Safari selbst ungemessen. |
| Safari (macOS/iOS) | **offen**. Ungeprüfter Hinweis aus Vorwissen: Safari räumt skriptgeschriebenen Speicher (IDB, SW-Registrierung) nach 7 Tagen ohne Nutzerinteraktion (ITP) – betrifft beide Wege, weil die Artefakte in IDB liegen; Messung an echtem Safari nötig. |
| iOS-WKWebView / Android-WebView | **offen**, kein Gerät/Simulatorlauf in diesem Strang. Ungeprüfter Hinweis: WKWebView erlaubt SW nur mit App-Bound Domains; ohne diese greift der Blob-Weg. |
| Electron / Workjet Ops | **offen**. Kein Electron-Binary auf diesem Rechner gefunden; Workjet.app darf ein Worker nicht starten (Regel). Nächster Schritt: `measure.mjs` mit `playwright._electron` gegen ein Electron-Binary des Workjet-Repos. |
| Firefox | nicht beauftragt, ungemessen. |

## Änderungen je App beim Port

Zählung im Klon @ 4794af03d, `src/apps/business-os/modules/` (40 Apps, ohne Tests):

| Muster heute | Treffer | SW-Weg | Blob-Weg |
|---|---|---|---|
| `import … from '../../shared/x.js'` | 80 in 36 Apps | → `'ctox:shared/x.js'` | → `'ctox:shared/x.js'` |
| relative Importe `from './x.js'` | 61 in 16 Apps | unverändert | vom Loader umgeschrieben |
| `new URL('<literal>', import.meta.url)` | 82 in 35 Apps | unverändert | vom Loader umgeschrieben |
| `new URL(\`…${x}…\`, import.meta.url)` | 10 in 7 Apps | unverändert | **umbauen** auf `api.shell.assetUrl` / `resolver` |
| `import.meta.url` sonst (z. B. `loadModuleMessages(import.meta.url, …)`, `split('?v=')`) | ~96 von 178 in 40 Apps | `?v=`-Auswertung entfernen, Rest unverändert | **umbauen** (als Wert unbrauchbar) |
| Cache-Buster `?v=${…}` | 51 in 27 Apps | entfernen (Inhaltsadressierung) | entfernen |
| `import('<literal>')` | 13 in 6 Apps | unverändert | vom Loader umgeschrieben |
| `import(<berechnet>)` | 1 in 1 App | unverändert | **umbauen** |
| relative `url()` in CSS | 2 | unverändert | vom Loader umgeschrieben |
| CSS-Scoping | – | `[data-app]` + Versionstrennung (Befund b) | dito |
| Import-Zyklen | ungezählt | unverändert | **nicht darstellbar** (Loader wirft) |

Gemeinsam für beide Wege: Einstieg `mount(root, api, {restored})` → `{unmount, snapshot}` (Entwurf §12).

## Lücken

- Safari, iOS-/Android-WebView, Electron/Workjet ungemessen (siehe oben).
- Harter Neuladen und SW-Neustart nur auf Chromium (CDP) gemessen.
- Netzweg (Spur 4) nicht enthalten: gemessen ab „Artefakte in IDB“. Speicherverbrauch der Blob-URLs (nur `revoke` bei
  `unmount`, nicht beim Swap) nicht gemessen.
- Messungen unter Fremdlast der Maschine; Spannweiten statt Einzelwerten angegeben.
- Die Loader-Warnungszahl 25 im Blob-Weg (Szenario g) zählt die `import.meta.url`-Stelle in `probe()` der Demo je Version.

## Messwerte letzter Lauf (Lauf 4, automatisch)

<!-- MESSWERTE:BEGIN (automatisch von measure.mjs, nicht von Hand aendern) -->
Stand: 2026-10-15T13:53:43.589Z · Rohdaten: `results.json`

| Szenario | Weg | Browser | Ergebnis | Messwerte / Befund |
|---|---|---|---|---|
| a Erstinstallation | sw | chromium | ok | init (SW-Registrierung+Kontrolle bzw. Loader) 10.1 ms; IDB-Schreiben shell 1.2 ms + demo 0.2 ms; Montage (import+fetch+css+img) 7.1 ms (davon import 3.7 ms); 2. Fenster gleiche Version 2 ms; css/img/locale/greet/relativer Import ok=true |
| b Aktivierung v2 (Live-Remount) | sw | chromium | ok | erster Wechsel v1->v2 3.4 ms, Eingabe erhalten=true; frische Versionen (neue URLs, 20x): p50 3.9 ms / p95 4.5 ms / max 5 ms; warmer Wechsel v1<->v2 (Modul-Map, 20x): p50 1 / p95 1.3 ms; IDB-Installation je Version p50 0.3 ms; altes v1-Modul unberuehrt=true (mountCount 2, greet 'Hallo x (shell v1)'); CSS-Farbe des alten v1-Fensters neben v-neu: rgb(0, 0, 200) |
| c Shell-Wechsel v1->v2 | sw | chromium | ok | neuer Mount gleiche App-Version (demo@v1, schon im Dokument geladen): (shell v1); neuer Mount neue App-Version: (shell v2); Vorschlag URL mit ~shell@v2: (shell v2); neues Dokument demo@v1: (shell v2); altes Fenster bleibt: (shell v1) |
| e Manipulierte Datei | sw | chromium | ok | Montage abgelehnt=true (Failed to fetch dynamically imported module: http://127.0.0.1:50104/~a/demo@bad/index.js); Code ausgefuehrt=false; Status manipuliert=404, unbekannte Version=404, unbekannte Datei=404 |
| g Berechnete Pfade (Template-Literal, import.meta.url als Wert) | sw | chromium | ok | new URL(`./locales/${lang}.json`, import.meta.url): dyn-ok; loadModuleMessages(import.meta.url): dyn-ok; Loader-Warnungen: 0 |
| h Last (App 41 Module, Shell 30 shared-Module) | sw | chromium | ok | App 1010 KB, Shell 1503 KB (IDB-Schreiben 6.3 ms); erster Mount (Shell+App kalt) 41.8 ms; weitere 5 frische App-Versionen (App kalt, Shell warm): p50 22 ms / max 28 ms |
| f Zwei Tabs | sw | chromium | ok | Tab 2 folgt Aktivierung (BroadcastChannel -> Remount), 10x: p50 6.1 ms / p95 7.3 ms; gleichzeitiges Montieren neue Version in beiden Tabs: 4.7 / 5 ms, beide ok=true |
| d Server gestoppt | sw | chromium | ok | Wechsel v1/v2/v1/v2: 1.9 / 1.2 / 1.4 / 0.9 ms; neue Version nach Stopp installiert+montiert: 4 ms; SW gestoppt+Neustart ohne Netz: ok 10.2 ms; Server-Treffer unter /~a/ waehrend des Laufs: 0 |
| a Erstinstallation | blob | chromium | ok | init (SW-Registrierung+Kontrolle bzw. Loader) 1.6 ms; IDB-Schreiben shell 2.3 ms + demo 0.5 ms; Montage (import+fetch+css+img) 10.1 ms (davon import 6.1 ms); 2. Fenster gleiche Version 0.9 ms; css/img/locale/greet/relativer Import ok=true |
| b Aktivierung v2 (Live-Remount) | blob | chromium | ok | erster Wechsel v1->v2 5.3 ms, Eingabe erhalten=true; frische Versionen (neue URLs, 20x): p50 5 ms / p95 7.6 ms / max 7.7 ms; warmer Wechsel v1<->v2 (Modul-Map, 20x): p50 1.1 / p95 1.8 ms; IDB-Installation je Version p50 0.6 ms; altes v1-Modul unberuehrt=true (mountCount 2, greet 'Hallo x (shell v1)'); CSS-Farbe des alten v1-Fensters neben v-neu: rgb(0, 0, 200) |
| c Shell-Wechsel v1->v2 | blob | chromium | ok | neuer Mount gleiche App-Version (demo@v1, schon im Dokument geladen): (shell v2); neuer Mount neue App-Version: (shell v2); neues Dokument demo@v1: (shell v2); altes Fenster bleibt: (shell v1) |
| e Manipulierte Datei | blob | chromium | ok | Montage abgelehnt=true (Hash falsch: demo@bad/util.js); Code ausgefuehrt=false; Status manipuliert=n/a, unbekannte Version=n/a, unbekannte Datei=n/a |
| g Berechnete Pfade (Template-Literal, import.meta.url als Wert) | blob | chromium | FEHLER | new URL(`./locales/${lang}.json`, import.meta.url): FEHLER:Failed to construct 'URL': Invalid URL; loadModuleMessages(import.meta.url): FEHLER:Failed to construct 'URL': Invalid URL; Loader-Warnungen: 25 |
| h Last (App 41 Module, Shell 30 shared-Module) | blob | chromium | ok | App 1010 KB, Shell 1503 KB (IDB-Schreiben 13 ms); erster Mount (Shell+App kalt) 54.3 ms; weitere 5 frische App-Versionen (App kalt, Shell warm): p50 30.1 ms / max 38.1 ms |
| f Zwei Tabs | blob | chromium | ok | Tab 2 folgt Aktivierung (BroadcastChannel -> Remount), 10x: p50 5.5 ms / p95 6.6 ms; gleichzeitiges Montieren neue Version in beiden Tabs: 5.5 / 4.8 ms, beide ok=true |
| d Server gestoppt | blob | chromium | ok | Wechsel v1/v2/v1/v2: 4.9 / 4.8 / 1.1 / 1.5 ms; neue Version nach Stopp installiert+montiert: 5.2 ms; SW gestoppt+Neustart ohne Netz: n/a; Server-Treffer unter /~a/ waehrend des Laufs: 0 |
| a Erstinstallation | sw | webkit | ok | init (SW-Registrierung+Kontrolle bzw. Loader) 86 ms; IDB-Schreiben shell 22 ms + demo 2 ms; Montage (import+fetch+css+img) 63 ms (davon import 18 ms); 2. Fenster gleiche Version 10 ms; css/img/locale/greet/relativer Import ok=true |
| b Aktivierung v2 (Live-Remount) | sw | webkit | ok | erster Wechsel v1->v2 30 ms, Eingabe erhalten=true; frische Versionen (neue URLs, 20x): p50 33 ms / p95 71 ms / max 86 ms; warmer Wechsel v1<->v2 (Modul-Map, 20x): p50 7 / p95 14 ms; IDB-Installation je Version p50 3 ms; altes v1-Modul unberuehrt=true (mountCount 2, greet 'Hallo x (shell v1)'); CSS-Farbe des alten v1-Fensters neben v-neu: rgb(0, 0, 200) |
| c Shell-Wechsel v1->v2 | sw | webkit | ok | neuer Mount gleiche App-Version (demo@v1, schon im Dokument geladen): (shell v1); neuer Mount neue App-Version: (shell v2); Vorschlag URL mit ~shell@v2: (shell v2); neues Dokument demo@v1: (shell v2); altes Fenster bleibt: (shell v1) |
| e Manipulierte Datei | sw | webkit | ok | Montage abgelehnt=true (Importing a module script failed.); Code ausgefuehrt=false; Status manipuliert=404, unbekannte Version=404, unbekannte Datei=404 |
| g Berechnete Pfade (Template-Literal, import.meta.url als Wert) | sw | webkit | ok | new URL(`./locales/${lang}.json`, import.meta.url): dyn-ok; loadModuleMessages(import.meta.url): dyn-ok; Loader-Warnungen: 0 |
| h Last (App 41 Module, Shell 30 shared-Module) | sw | webkit | ok | App 1010 KB, Shell 1503 KB (IDB-Schreiben 68 ms); erster Mount (Shell+App kalt) 195 ms; weitere 5 frische App-Versionen (App kalt, Shell warm): p50 67 ms / max 119 ms |
| f Zwei Tabs | sw | webkit | ok | Tab 2 folgt Aktivierung (BroadcastChannel -> Remount), 10x: p50 31 ms / p95 88 ms; gleichzeitiges Montieren neue Version in beiden Tabs: 27 / 25 ms, beide ok=true |
| d Server gestoppt | sw | webkit | ok | Wechsel v1/v2/v1/v2: 7 / 5 / 8 / 11 ms; neue Version nach Stopp installiert+montiert: 31 ms; SW gestoppt+Neustart ohne Netz: n/a; Server-Treffer unter /~a/ waehrend des Laufs: 0 |
| a Erstinstallation | blob | webkit | ok | init (SW-Registrierung+Kontrolle bzw. Loader) 27 ms; IDB-Schreiben shell 18 ms + demo 3 ms; Montage (import+fetch+css+img) 50 ms (davon import 9 ms); 2. Fenster gleiche Version 28 ms; css/img/locale/greet/relativer Import ok=true |
| b Aktivierung v2 (Live-Remount) | blob | webkit | ok | erster Wechsel v1->v2 40 ms, Eingabe erhalten=true; frische Versionen (neue URLs, 20x): p50 23 ms / p95 102 ms / max 117 ms; warmer Wechsel v1<->v2 (Modul-Map, 20x): p50 6 / p95 39 ms; IDB-Installation je Version p50 3 ms; altes v1-Modul unberuehrt=true (mountCount 2, greet 'Hallo x (shell v1)'); CSS-Farbe des alten v1-Fensters neben v-neu: rgb(0, 0, 200) |
| c Shell-Wechsel v1->v2 | blob | webkit | ok | neuer Mount gleiche App-Version (demo@v1, schon im Dokument geladen): (shell v2); neuer Mount neue App-Version: (shell v2); neues Dokument demo@v1: (shell v2); altes Fenster bleibt: (shell v1) |
| e Manipulierte Datei | blob | webkit | ok | Montage abgelehnt=true (Hash falsch: demo@bad/util.js); Code ausgefuehrt=false; Status manipuliert=n/a, unbekannte Version=n/a, unbekannte Datei=n/a |
| g Berechnete Pfade (Template-Literal, import.meta.url als Wert) | blob | webkit | FEHLER | new URL(`./locales/${lang}.json`, import.meta.url): FEHLER:"./locales/de.json" cannot be parsed as a URL.; loadModuleMessages(import.meta.url): FEHLER:"./locales/de.json" cannot be parsed as a URL.; Loader-Warnungen: 25 |
| h Last (App 41 Module, Shell 30 shared-Module) | blob | webkit | ok | App 1010 KB, Shell 1503 KB (IDB-Schreiben 29 ms); erster Mount (Shell+App kalt) 174 ms; weitere 5 frische App-Versionen (App kalt, Shell warm): p50 76 ms / max 118 ms |
| f Zwei Tabs | blob | webkit | ok | Tab 2 folgt Aktivierung (BroadcastChannel -> Remount), 10x: p50 15 ms / p95 66 ms; gleichzeitiges Montieren neue Version in beiden Tabs: 35 / 21 ms, beide ok=true |
| d Server gestoppt | blob | webkit | ok | Wechsel v1/v2/v1/v2: 18 / 7 / 3 / 6 ms; neue Version nach Stopp installiert+montiert: 12 ms; SW gestoppt+Neustart ohne Netz: n/a; Server-Treffer unter /~a/ waehrend des Laufs: 0 |
<!-- MESSWERTE:END -->
