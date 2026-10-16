# Strang MONACO: Code-Ansicht mit echtem Editor in den Laboren

Stand 16.10.2026, Klon `~/Documents/ctox-shell-v4` (Zweig `shell-v4/prototype`, Basis `7750b5a37` plus
ungesicherte Arbeit anderer Stränge). Kein Commit, kein git außer Lesen.

## Ergebnis in einem Satz

Die Labore liefen auf dem textarea-Rückfall, weil sie den Shell-Code aus **ungebundenen Rohpfaden** laden. Der echte
Ladeweg bindet die Shell-Instanz an `~lib:monaco@<M.m>` und löst `ctox:lib/monaco@0/` damit auf. Beide Labore laden
den Quelltext-Modus jetzt von derselben Instanz-URL wie der echte Ladeweg. Diese URL baut der Kern-Stub
(`artifacts.bind`), die Umschreibung leisten die Funktionen aus `v4/sw.js`. Die Code-Ansicht läuft dort mit Monaco.
Im Monaco-Ladeweg selbst habe ich keinen Fehler gefunden. Eine Hypothese zum Editor-Worker im echten Weg steht unten
unter „Offen“; sie ist ungeprüft.

## 1. Ursache

`v4/source/editor.js` enthält das Literal `const MONACO_LIB_BASE = 'ctox:lib/monaco@0/'`.

- **Echter Ladeweg:** Der Kernel montiert die Shell unter `kernel.artifacts.bind('shell', sv)`. Die Wurzel ist
  `/~a/shell@<sv>~lib:ctox-office@<M.m>~lib:monaco@<M.m>/`, weil das Shell-Manifest
  `libs: {ctox-office: "1", monaco: "0"}` trägt (`v4/manifests/shell.manifest.json`; Stub `makeBinding`/`resolveLibs`).
  `frame/index.js` lädt `../source/source-mode.js` und darüber `editor.js` relativ, erbt also diese Wurzel. Der SW
  (`serve()` → `rewriteShellRefs`/`rewriteLibRefs`) ersetzt das Literal durch
  `'/~a/lib:monaco@<M.m>~lib:ctox-office@<M.m>/'`. `new URL(…, location.origin)` ergibt eine Adresse auf dem eigenen
  Origin, und `loader.js`, `editor.main.*` und `codicon.ttf` liefert `serveLib()`.
- **Labore:**
  - Das Quelltext-Labor lud `/business-os/v4/source/…`.
  - Das Rahmen-Labor lud `/v4/frame/index.js`.
  - Das Geometrie- und das Rundgang-Labor laden ebenfalls `/v4/frame/index.js`.

  In keinem Labor gab es einen SW oder eine ~lib:-Bindung, das Literal blieb also stehen. `new URL('ctox:lib/…', origin)`
  ergibt eine `ctox:`-URL, `fetch` meldet „URL scheme "ctox" is not supported“, und `loadMonaco()` wirft
  „monaco nicht lokal“. `createEditor` fängt das still ab und nimmt die textarea. Das Rahmen-Labor hatte genau diese
  Meldung als „bekannt“ aus der AG-Fehlerzählung herausgefiltert.

Dem Labor fehlte also weder das Lib-Artefakt (`vendor/monaco` liegt vor) noch eine Import-Map. Es fehlte die
**Instanz-URL der Shell** und die **Umschreibung des SW**.

## 2. Lösung: derselbe Mechanismus wie im echten Ladeweg, ohne Labor-Sonderweg

Neue Datei `v4/lab/sw-lab.mjs`. Sie liefert gebundene Instanz-URLs ohne SW so aus, wie der SW es tut:

- Grammatik, Bindung und Umschreibung kommen aus `v4/sw.js` selbst: `URL_RE`, `LIB_URL_RE`, `parseBinding`, `rootOf`,
  `decodePath`, `rewriteShellRefs` und `rewriteLibRefs`, per `vm` geladen wie in `ctox-lib.test.mjs`. Das ist keine
  Kopie.
- `/~a/shell@<sv>[~lib:…]*/<pfad>` liefert die Datei `business-os/<pfad>` aus dem Shell-Quellbaum.
- `/~a/lib:<name>@<M.m>[~lib:…]*/<pfad>` liefert die Datei aus dem Quellordner der Bibliothek, so wie er in
  `standard-libs.json` steht (nur Einträge mit Status „approved“, sonst 404 wie im SW).
- Eine mehrdeutige Bindung oder eine unbekannte Bibliothek ergibt 404.
- Nicht nachgebildet sind IDB-Quelle, sha256-Prüfung und Kernel-Anforderung. Die prüfen weiterhin
  `tools/isolation/sw-lib-miss.test.mjs` und `v4/lab/ladeweg`.

Die URL baut der Kern-Stub, nicht das Labor. Die Laborseite veröffentlicht `lib:ctox-office@1.0`, `lib:monaco@0.0`
und die Shell `lab` mit `libs` (wie das Geometrie-Labor K-1d). Danach lädt sie den Shell-Code über
`kernel.artifacts.bind('shell', 'lab').url(<pfad>)`.

| Labor | Änderung |
|---|---|
| Quelltext-Labor `source/lab/server.mjs` | Route für Instanz-URLs über `sw-lab.mjs`; andere `/~a/`-Pfade ergeben 404 |
| Quelltext-Labor `source/lab/lab.js` | `chrome-toggle.js`, `upload.js` und `schema-diff.js` kommen von `/~a/shell@lab~lib:ctox-office@1.0~lib:monaco@0.0/v4/source/…`; `__lab.shellRoot` zeigt die Wurzel |
| Rahmen-Labor `frame/lab/serve.mjs` | dieselbe Route; Pfade unter `/~a/shell@` und `/~a/lib:`, die keine Instanz-URL sind, ergeben 404; App-Pfade unverändert |
| Rahmen-Labor `frame/lab/stub-lab.html` | Parameter `shell=bound` lädt den ganzen Rahmen von `artifacts.bind('shell','lab').url('v4/frame/index.js')`; ohne den Parameter bleibt es beim Rohpfad `../index.js` (Begründung unten) |
| Rahmen-Labor `frame/lab/frame.test.mjs` (nur AG-Block) | AG läuft mit `shell=bound`; neuer Fall AG-M; die Ausnahme für die Monaco-Meldung ist weg; Tastatureingaben in AG-3, AG-4 und AG-4c laufen editorunabhängig über `caret()` (Monaco per Tastatur wie ein Nutzer) |

Warum nicht alle Rahmen-Fälle gebunden laufen: Viele Fälle sprechen Module per `import('/v4/shell/sync-state.js')`
oder `import('/v4/frame/topbar.js')` als **dieselbe Modulinstanz** wie den Rahmen an. FR2 prüft außerdem die
Stylesheet-Pfade wörtlich (`/v4/frame/theme/theme.css,…`). Gebunden wären das andere Instanzen, und diese Fälle würden
rot, ohne dass im Rahmen etwas falsch ist. Diese Fälle umzustellen ist eine eigene Karte (siehe „Offen“).

## 3. Abnahme (selbst gelaufen, 16.10.2026 00:41 bis 01:00)

| Lauf | Ergebnis |
|---|---|
| `node v4/source/lab/accept.mjs --editor auto` | 13/13, `editorKind: "monaco"` (vorher `textarea`), errors `[]`; `results.json`, `shots/2-editor-1280.png` mit Monaco (Syntaxfarben, Zeilennummern) |
| `node v4/source/lab/accept.mjs --editor textarea` | 13/13, errors `[]` (`results-textarea.json`) |
| `CTOX_APP_API_STUB=…/8489b4e AG_ONLY=1 node v4/frame/lab/frame.test.mjs stub`, vorher | 10/10, aber mit textarea (Monaco-Meldung herausgefiltert) |
| derselbe Lauf, nachher | **11/11**, AG-M: Wurzel `/~a/shell@lab~lib:ctox-office@1.0~lib:monaco@0.0/`, editor.js von dort, `loader.js` von `/~a/lib:monaco@0.0~lib:ctox-office@1.0/vs/loader.js`, `editor: monaco`, 18 Zeilen gerendert; AG-1 bis AG-7, Einklappen und „ohne Seitenfehler“ grün (jetzt ohne Ausnahme) |
| Gegenprobe ohne `shell=bound` (gleiche Seite, Rohpfad) | Konsole „Fetch API cannot load ctox:lib/monaco@0/vs/loader.js“, Editor = `textarea`; AG-M wäre dort rot |
| `node --test tools/isolation/ctox-lib.test.mjs tools/isolation/sw-lib-miss.test.mjs` | 23/23 (sw.js unverändert) |
| Monaco-Dateien beim Öffnen gegen `manifests/lib-monaco.manifest.json` | 7 Pfade angefordert (loader, editor.main.js/.css/.nls.js, basic-languages/javascript, language/typescript/tsMode, codicon.ttf), alle im Manifest, also im echten Weg vom SW lieferbar |
| `CTOX_APP_API_STUB=…/8489b4e node v4/frame/lab/frame.test.mjs stub` (voll, 00:47 bis 00:58, während LOOP/DARK im Baum arbeiteten) | **205/205**, Exit 0 |

Selbst angesehene Bilder bei 1280 (`frame/lab/shots/`):

- `integrated-stub-ag-panel-1280.png`: Code-Ansicht „Notizen“ mit Monaco (Syntaxfarben, Zeilennummern) und offener
  Agent-Seitenleiste.
- `integrated-stub-ag-diff-1280.png`: Diff nach dem Agent-Lauf (+1 Zeile, „14 unveränderte Zeilen“) neben der
  Seitenleiste mit Antwort und Dateiliste.
- `integrated-stub-ag-conflict-1280.png`: Monaco mit den getippten eigenen Zeilen 1 bis 3 und
  „Änderungen übernehmen“ in der Seitenleiste.

Zwei Laborfehler, die erst mit Monaco sichtbar wurden, sind behoben. Beide lagen in der Messung, nicht in der Shell:

1. Das Bild `ag-panel` entstand, bevor der Editor fertig war. Monaco ist asynchron, die textarea war sofort da. Das
   Bild wartet jetzt auf `.v4-src[data-editor] .v4-src-ed` und `.view-line`; das dauerte gemessen 241 ms.
2. Der Klick in die Mitte von `.view-lines` ließ Playwright per scrollIntoView den inneren Container seitlich
   verschieben, an Monaco vorbei. Danach war nur noch das Zeilenende zu sehen. Jetzt wird an Position (80, 8) in die
   sichtbare Editorfläche geklickt.

## 4. Echter Fehler im Ladeweg?

Für Monaco selbst: **keiner gefunden**. Im echten Weg ist die Shell immer gebunden, weil der Kernel `bind` aufruft,
und der SW schreibt das Literal um (`ctox-lib.test.mjs` deckt genau `MONACO_BASE` ab). Alle Dateien, die Monaco beim
Öffnen lädt, stehen im Lib-Manifest. Deshalb gibt es keinen Ladeweg-Fix und keinen „vorher roten“ Test. Der rote
Vorher-Zustand liegt im Labor: AG-M ist ohne Bindung rot (Gegenprobe oben).

## Offen (nicht in diesem Strang behoben, Begründung je Punkt)

1. **Editor-Worker im echten SW-Weg (Hypothese, ungeprüft).** `editor.js` startet den Monaco-Worker als `data:`-URL
   mit `importScripts('<origin>/~a/lib:monaco@…/vs/base/worker/workerMain.js')`. Ein `data:`-Worker hat einen opaken
   Origin und wird von keinem Service Worker kontrolliert. Der Abruf geht also ans Netz, und dort gibt es kein `/~a/`.
   Monaco fiele dann mit einer Konsolenwarnung („Could not create web worker(s)“) auf den Hauptthread zurück. Im Labor
   gibt es keinen SW, dort beantwortet der Server den Abruf; der Fall ist hier also nicht sichtbar. Ein gleich-origin
   Worker wäre zwar SW-kontrolliert. Dann ginge aber eine `ctox:lib-miss`-Anforderung an den Worker-Client
   (`event.clientId`), wo kein Kernel lauscht, und der SW wartet ohne eigenen Timeout. Das ist eine SW-Vertragsfrage,
   kein Labor-Fix.
   *Entscheidender Test:* im SW-Labor `v4/lab/ladeweg` die Code-Ansicht öffnen und auf die Worker-Warnung prüfen sowie
   prüfen, ob `workerMain.js` einen `lib-miss` auslöst.
2. **Modulidentität von `shared/` bei gebundener Shell-Wurzel (Beobachtung).** Der Kopfkommentar von `v4/sw.js` sagt,
   die Importe des Rahmens lägen unter `/~a/shell@<sv>/shared/…`, ohne ~lib:. Der Kern-Stub baut die Shell-Wurzel aber
   mit ~lib:. Die relativen Importe des Rahmens laden darum `/~a/shell@lab~lib:…/shared/window-manager.js` usw.
   (gemessen, 11 Dateien), Apps über `ctox:shared/` dagegen `/~a/shell@lab/shared/…`. Gemeinsam genutzte shared-Module
   werden dann zweimal geladen. Ein Zustandsfehler daraus ist nicht belegt: in `context-chat.js`, `window-manager.js`
   und `taskbar-pins.js` stehen auf Modulebene nur Caches. Entscheiden müssen das die Eigentümer von SW und Kern:
   entweder die Shell-Wurzel ohne ~lib: binden und Bibliotheken anders zuordnen, oder den Kommentar korrigieren.
3. **Restliche Rahmen- und Geometrie-Fälle gebunden laufen lassen.** Dazu müssten die `import('/v4/…')`-Sonden in
   `frame.test.mjs` und die wörtlichen Stylesheet-Pfade auf `__lab.shellRoot` umgestellt werden. Danach kann
   `stub-lab.html` immer gebunden laden. Das Geometrie- und das Rundgang-Labor (`tools/geometry`, `tools/rundgang`)
   haben dieselbe Ursache, liegen aber außerhalb meiner Dateien.

## Dateien

- neu: `v4/lab/sw-lab.mjs`, `v4/source/MONACO.md`
- geändert: `v4/source/lab/server.mjs`, `v4/source/lab/lab.js`, `v4/frame/lab/serve.mjs`,
  `v4/frame/lab/stub-lab.html`, `v4/frame/lab/frame.test.mjs` (nur AG-Block)
- Laborergebnisse: `v4/source/lab/results.json`, `results-textarea.json`, `v4/source/lab/shots/*`,
  `v4/frame/lab/shots/integrated-stub-ag-*`
- nicht angefasst: `v4/sw.js`, `v4/blob-loader.js`, `v4/source/editor.js`, `v4/tools/node/ctox-shared.mjs`
  (die Node-Auflösung lädt Monaco nicht; Node-Tests importieren `editor.js` nicht)


---

# Runde 2 (§16.50): Worker gleich-ursprünglich, Worker-Miss im SW, shared genau einmal

Stand 16.10.2026 01:20. Kern-Entscheidung §16.50: beide Befunde aus „Offen“ (Nr. 1 Worker, Nr. 2 shared-Identität)
sind entschieden, die SW-Seite gehört diesem Strang. Kein git.

## Umsetzung

**`v4/source/editor.js`** (drei gezielte Edits, LOOP arbeitet parallel in `v4/source`):

- Neues Literal `MONACO_WORKER_MAIN = 'ctox:lib/monaco@0/vs/base/worker/workerMain.js'`. Der SW schreibt es mit
  derselben Funktion wie `MONACO_LIB_BASE` um (`rewriteLibRefs`), auf die gebundene Lib-Wurzel
  `/~a/lib:monaco@<M.m>~lib:…/vs/base/worker/workerMain.js`. Damit gibt es keine String-Bastelei in der Shell.
- `getWorkerUrl` liefert genau diese URL, kein `data:` und kein `blob:` mehr. Ist die URL nicht gleich-ursprünglich,
  wird Monaco nicht geladen; dann gilt der textarea-Rückfall.
- `createEditor(…, { workerUrl })` nimmt optional `binding.workerUrl(lib, path)` (K-1j) an, sobald der Kernel es
  liefert. Bis dahin gilt das umgeschriebene Literal.
- Monaco 0.32 (AMD) hat keine `editor.worker.js`. Einstieg jedes Monaco-Workers ist `vs/base/worker/workerMain.js`.
  Es lädt Editor- und Sprach-Worker (`vs/language/typescript/tsWorker.js`) relativ zur eigenen URL, also unter
  derselben gebundenen Wurzel.
- Weitere Worker-Erzeuger gibt es in `v4/`, `shared/` und `modules/` nicht (grep nach `new Worker`, `getWorkerUrl`
  und `data:`-Skripten).

**`v4/sw.js`:**

- **Worker-Miss:** `kernelClient(clientId)` nimmt den anfragenden Client nur, wenn er ein Fenster ist. Sonst
  wählt es aus `clients.matchAll({type:'window'})` in dieser Reihenfolge:
  1. das Leiter-Fenster,
  2. ein anderes gemeldetes Kernel-Fenster,
  3. das fokussierte Fenster,
  4. das erste Fenster.

  Kernel-Fenster melden sich mit `postMessage({type:'ctox:kernel', leader})`. Dieser Vertrag ist neu und steht im
  Kopfkommentar. Ohne Meldung gilt jedes Fenster; der Kernel muss die Meldung bei Start und bei jedem Leiterwechsel
  senden. Treffermeldungen (`ctox:lib-hit`) aus Workern gehen ebenfalls an das Kernel-Fenster.
- **Frist:** Eine Anforderung wartet höchstens `LIB_MISS_TIMEOUT_MS = 15000` auf ein Fenster (Abfrage alle 250 ms)
  und auf dessen Antwort. Danach kommt 504 mit `Cache-Control: no-store`, und der Body nennt den Grund, z. B.
  `kein Fenster mit Kernel binnen 15000 ms (lib:monaco@0.32/vs/a.js)` oder `keine Kernel-Antwort binnen …`.
  Ein Kernel-Abbruch bleibt 404. Verhaltensänderung: „kein Client erreichbar“ ergab früher sofort 404, jetzt 504
  nach der Frist.
- **shared:** `ctox:shared/` wird auf die exakt gebundene Shell-Wurzel umgeschrieben,
  `/~a/shell@<sv>` plus `bindingSuffix(Bindung der URL)` plus `/shared/<x>`, also samt `~lib:`-Segmenten, sortiert.
  Für Shell-Dateien ist das genau die Wurzel des Rahmens; die relativen Importe `../../shared/x.js` und
  `ctox:shared/x.js` ergeben dieselbe URL und damit eine Modulinstanz. Kopfkommentar angepasst.
  Grenze (im Kommentar): Eine App-Instanz mit anderer Lib-Bindung als die Shell (zusätzliche Bibliothek, neuere
  Minor) bekommt folgerichtig eigene shared-Instanzen. Dass Apps mit der Lib-Bindung der Shell montiert werden,
  muss der Kernel sichern.

**Labore:**

- `v4/lab/sw-lab.mjs` nutzt die Funktionen aus `sw.js`. Rahmen- und Quelltext-Labor übernehmen die neue
  shared-Form deshalb automatisch.
- Das SW-Labor `v4/lab/ladeweg` hat neu `monaco.html`, `monaco-lab.js` und `monaco.mjs`. In `server.mjs` kommen
  zwei Pfade dazu: `/~stub/` für den Kern-Stub und `/__spur4/` als Ersatz für Spur 4 des Labor-Kernels, auf
  Positivliste. In `kernel-attrappe.js` kommen `putFileList` und `putArtifact` dazu.

## Abnahme (selbst gelaufen)

| Lauf | Ergebnis |
|---|---|
| `node --test tools/isolation/ctox-lib.test.mjs tools/isolation/sw-lib-miss.test.mjs` | **28/28** (vorher 23; neu: shared genau einmal, Worker-Miss mit Fenster, Leiter-Vorrang/Leiterwechsel, Worker-Miss ohne Fenster → 504 + spätes Fenster, Fenster antwortet nicht → 504; Frist im Test per Quelltext-Ersatz auf 200–1500 ms verkürzt, Standard 15000 geprüft) |
| Mutation `SW_MUTATE=worker-own` (Anforderung geht an den Worker, alter Stand) | rot: Worker-Miss mit Fenster, ohne Fenster, Leiter (3) |
| `SW_MUTATE=no-timeout` (Fenstersuche ohne Frist) | rot: „kein Fenster binnen Frist → 504“ und „Worker-Miss ohne Fenster“ (Test-Timeout) |
| `SW_MUTATE=no-reply-timeout` (keine Frist auf die Kernel-Antwort) | rot: „Fenster antwortet nicht → 504“ |
| `SW_MUTATE=no-leader` (Leiter-Meldung ignoriert) | rot: Leiter-Vorrang |
| `SW_MUTATE=shared-bare` (shared ohne `~lib:`, alter Stand) | rot: „shared genau einmal geladen“ (zwei URLs je Datei) und App-Datei-Fall |
| `node --test tools/geometry/geometry-v4.test.mjs tools/isolation/check-isolation.test.mjs` | 35/35 (nutzen die SW-Umschreibung mit) |
| **SW-Labor** `node v4/lab/ladeweg/monaco.mjs` | **PASS**: Seite vom SW kontrolliert, Code-Ansicht von `/~a/shell@lab~lib:ctox-office@1.0~lib:monaco@0.0/v4/source/source-mode.js` (10 Shell-Dateien aus IDB), Editor = monaco, 3 Worker alle `http://127.0.0.1:<port>/~a/lib:monaco@0.0~lib:ctox-office@1.0/vs/base/worker/workerMain.js`, `tsWorker.js` per Lib-Miss aus dem Worker vom Kernel-Fenster beantwortet, 9 Misses und 9 Treffer, **keine Warnung „falling back to main thread“**, 0 Server-Treffer unter `/~a/`, keine Fehler; ca. 3 s |
| SW-Labor `MUT=editor-data` (alter `data:`-Worker) | FAIL wie erwartet: Warnung „Could not create web worker(s). Falling back to loading web worker code in main thread…“, `importScripts … failed to load`, 3 Server-Treffer unter `/~a/`. Das bestätigt die Hypothese „Offen Nr. 1“ aus Runde 1 im echten SW-Weg |
| SW-Labor `MUT=sw-worker-own` (SW schickt den Miss an den Worker) | FAIL wie erwartet: `tsWorker.js` erreicht das Kernel-Fenster nie, Worker-Fehler nach der 15-s-Frist (Laufzeit 27,8 s statt 3 s) |
| Quelltext-Labor `accept.mjs --editor auto` | 13/13, `editorKind: monaco`, errors `[]` |
| Rahmen-Labor `AG_ONLY=1 … stub` (8489b4e) | 11/11 (AG-M grün) |
| Rahmen-Labor voll stub (8489b4e), 01:05 bis 01:15 | **205/205**, Exit 0 |

Bild: `v4/lab/ladeweg/shots/monaco-1280.png` (selbst angesehen): Code-Ansicht „Demo“ mit Monaco, Syntaxfarben und
Zeilennummern. Die Agent-Seitenleiste fehlt in diesem Labor, weil sie die App-API der Shell aus dem Rahmen braucht.
Das Labor prüft den Lade- und Worker-Weg. Seitenleiste und Diff mit Monaco belegt das Rahmen-Labor (AG, Bilder
`integrated-stub-ag-*`).

## Offen nach Runde 2

1. **Kernel:** `postMessage({type:'ctox:kernel', leader})` an `navigator.serviceWorker.controller` bei Start und
   Leiterwechsel senden. Der Stub tut das noch nicht; ohne Meldung nimmt der SW das fokussierte oder erste Fenster.
   Außerdem `binding.workerUrl` (K-1j) liefern und von `source-mode.js` an `createEditor` durchreichen. Das ist die
   Datei von LOOP, ich habe sie nicht angefasst.
2. **Kernel:** Apps mit der Lib-Bindung der Shell montieren, sonst entstehen eigene shared-Instanzen je App (siehe
   Grenze oben).
3. `v4/blob-loader.js` (Rückfallweg ohne SW) kann Worker grundsätzlich nicht gleich-ursprünglich starten, weil
   Blob-URLs keine hierarchische Basis sind; dort bleibt Monaco ohne Worker. Das war schon vorher als Grenze
   dokumentiert und ist unverändert.
4. Die Ladeweg-Messung `measure.mjs` habe ich nicht neu gefahren, weil sie `results.json` und `RESULTS.md`
   überschreibt. Für URLs ohne `~lib:` ändert sich nichts; der Test deckt `/~a/shell@4.1/shared/a.js` ab.

## Dateien Runde 2

- geändert: `v4/sw.js`, `v4/source/editor.js`, `v4/tools/isolation/sw-lib-miss.test.mjs`,
  `v4/tools/isolation/ctox-lib.test.mjs`, `v4/lab/ladeweg/server.mjs`, `v4/lab/ladeweg/kernel-attrappe.js`
- neu: `v4/lab/ladeweg/monaco.html`, `v4/lab/ladeweg/monaco-lab.js`, `v4/lab/ladeweg/monaco.mjs`,
  `v4/lab/ladeweg/results-monaco*.json`, `v4/lab/ladeweg/shots/monaco*-1280.png`
