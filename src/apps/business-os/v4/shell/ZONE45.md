# ZONE45: ehrlicher Verbindungszustand (Meldungs-Badge und Zustandspunkt)

Anlass ist Owner-Mangel M-08. Auf welsch stand „CTOX PLATTFORM AKTIV“ grün, während Collections hingen. Ab jetzt gilt:
Grün gibt es nur, wenn jede angeforderte Collection nachweislich live ist. Fehlt der Nachweis, ist der Zustand
`unknown`, also grau und nie grün.

Vertrag: Kern §16.45 (V4 `kernel.metrics.collections`, `kernel.reconnect()`), dazu der V3-Rückfall über
`window.ctoxBusinessOsSyncDiagnostics`, nur lesend und nur bis Samstag.

## Dateien (Stand 15.10.2026, 21:27)

| Datei | Art | Inhalt |
|---|---|---|
| `v4/shell/sync-state.js` | neu | `createSyncState({kernel, requested?, appCollections?, diagnostics?, pollMs?})` mit `getOverallState()`, `getAppState(appId, collections?)`, `subscribe(fn)`, `reconnect()`, `canReconnect`, `setRequested`, `refresh`, `destroy`. Dazu `currentSyncState()` (für TOPBAR), `SYNC_TEXT`, `SYNC_CODE`, `v3CollectionState` |
| `v4/shell/sync-state.test.mjs` | neu | 23 Unit-Tests (node --test), davon 7 Fälle mit je einem automatischen Mutationsbeleg und 1 Lauf gegen den Kern-Stub |
| `v4/shell/status-zone.js` | ergänzt | `attachSyncState(zone, syncState, {appId, collections?, warnDelayMs})`: Eintrag `shell.sync` am Meldungs-Badge |
| `v4/frame/index.js` | 3 gezielte Stellen | Import (Z. 37–38), ein Sync-Zustand je Shell in `startShell` (Z. 256–274, Abbau über `state.unsubs`), Einhängen je App-Fenster in `openApp` (Z. 633–634) |
| `v4/frame/lab/frame.test.mjs` | ergänzt | Block `z45()` mit Fällen `Z45…` (mock und stub), `Z45_ONLY=1` startet nur diesen Block |

## Regeln, wie gebaut

- **Quelle:** zuerst V4 (`metrics.collections` vorhanden), sonst V3 (`window.ctoxBusinessOsSyncDiagnostics`). Die
  Diagnose darf eine Funktion oder ein Objekt sein. Gibt es keine Quelle, ist der Zustand `unknown`.
- **Stufen:** `error` (mindestens eine Collection im Fehler) > `warn` (catching_up oder offline) > `unknown` (kein
  Nachweis) > `ok`.
- **V4:** Angefordert wird mit dem Grundnamen (`notes`). Gezählt wird die höchste Generation (`notes@g2`). Meldet der
  Kernel eine angeforderte Collection nicht, gilt sie bei `link.state` offline als `offline`, sonst als `unknown`,
  nie als live.
- **V3:** Grün nur bei `connectionStatus==='connected' && queryReady===true`. Gelb bei `reconnecting`, gesetztem
  `reconnectingSince` oder `lastLifecycleEvent.code==='peer_connect_timeout'`, je Collection oder auf oberster Ebene.
  Gelb hat Vorrang vor Grün. **Über den Vertrag hinaus:** `connectionStatus` ∈ error | failed | stopped | role-denied
  ergibt `error`. Alles andere (connecting, follower, demand-only ohne queryReady) ergibt `unknown`.
- **Gesamtzustand (Zustandspunkt):** `requested` sind die Collections der Shell. Ist die Liste leer, zählen ALLE
  gemeldeten Collections, denn grün heißt „wirklich alles live“.
- **App-Zustand (Badge):** Die Collections einer App stehen in `collections` ihres Manifests
  (`kernel.artifacts.manifest`, je App und Version einmal geladen). Ohne Collections zählt nur die Verbindung.
- **Ereignisse:** V4 hängt am Abo von `kernel.metrics`, V3 am Fensterereignis `ctox-business-os-sync-diagnostics`.
  Dazu kommt eine Abfrage alle ≥ 1 s, aber nur ohne V4 und nur bei vorhandenen Abonnenten, weil V3 sein Objekt auch
  still ändert. Ohne Abonnenten gibt es kein Kernel-Abo.
- **Badge-Eintrag:** `shell.sync`, Quelle `shell`.
  - warn: „Verbindung wird wiederhergestellt“ erscheint erst, wenn der Zustand **länger als 2 s** anhält. Das ist
    dieselbe 2-s-Regel wie bei den Aktivierungszuständen, weil ein normaler Abgleich nach einer Änderung nur
    Millisekunden dauert. Der Zustandspunkt (`getOverallState`) wird dagegen **sofort** gelb.
  - error: „Verbindung unterbrochen“ erscheint sofort.
  - Aktion „Neu verbinden“ ruft `kernel.reconnect()` auf. Den Knopf gibt es nur, wenn der Kernel `reconnect` hat; V3
    ist nur lesend.
  - Ist alles live (oder `unknown`), verschwindet der Eintrag still.
  - Der Eintrag räumt sich selbst ab, wenn das Fenster zugeht (Zone `destroy`) oder die Shell getauscht wird
    (`state.sync.destroy()` in `state.unsubs`).

## Abnahme (selbst ausgeführt, 15.10.2026)

- `node --test src/apps/business-os/v4/shell/sync-state.test.mjs`: 23/23, Stub-Pin 74137b1 (Standard). Mit
  `CTOX_APP_API_STUB=…/68b83a3` ebenfalls 23/23; der Stub-Fall prüft dort den Zweig „ohne metrics.collections →
  unknown“.
  - Fälle mit Mutationsbeleg: 17 Collections/eine catching_up → warn · error → error · alles live → ok ·
    V3-Objekt mit peer_connect_timeout → warn · V3-Funktion (über `window.ctoxBusinessOsSyncDiagnostics`) → warn ·
    keine Quelle → unknown · reconnect ruft kernel.reconnect. Jede Mutante entsteht als Kopie des Moduls mit genau
    einer gebrochenen Regel, und der Test verlangt, dass der Fall gegen sie **rot** wird.
- `Z45_ONLY=1 … frame.test.mjs stub` mit Pin 74137b1: 8/8 (Z45-1 bis Z45-6, Z45-9, ohne Seitenfehler).
- `Z45_ONLY=1 … frame.test.mjs stub` mit Pin 68b83a3: 3/3 (unknown, nie ok, kein Badge-Eintrag; Leck; ohne Fehler).
- `Z45_ONLY=1 … frame.test.mjs mock`: 9/9 (zusätzlich Z45-7 V3 als Objekt und als Funktion, Z45-8 keine Quelle).
- Der Lab-Lauf hat einen echten Fehler gefunden, der dann behoben wurde: Ist die V3-Diagnose am Fenster eine
  Funktion, wurde sie nicht aufgerufen (Z45-7 rot). Jetzt wird sie entpackt, und der Unit-Fall prüft genau diesen Weg.
- Volle Läufe: siehe Abschnitt „Volle Rahmen-Labor-Läufe“ unten.
- Bilder: `v4/frame/lab/shots/integrated-[stub-]z45-warn-1280.png`, `…z45-error-1280.png`.

## Zusatz: FR6a-Befund „stub mit 74137b1 wirft, je Shell-Tausch +1 Observer“

- **Nicht nachstellbar im heutigen Baum.** Drei volle stub-Läufe mit 74137b1 (21:13, 21:15 und der Endlauf) liefen
  ohne Ausnahme („Lauf ohne Ausnahme“ nie rot). Die Observer-Zahl blieb über 5 Tausche konstant bei
  `28/21/null/1/3`. Z45-9 zählt zusätzlich ab VOR dem ersten Tausch: 15 → 15 → 15 → 15.
- **Ursache des „+1 je Tausch“ eingegrenzt (Probe mit verändertem `index.js`, nur über `page.route`, keine
  Dateiänderung):**
  - unverändert: 21 → 21 → 21 → 21 → 21
  - ohne `state.unsubs.push(() => taskbar.disconnect())`: 21 → 22 → 23 → 24 → **25**, also genau +1 je Tausch. Das
    ist der `ResizeObserver` der Taskleiste (`v4/frame/taskbar.js:264`), den nur `disconnect()` (`taskbar.js:344`)
    abbaut.
  - ohne den chromeRo-Abbau beim Tausch: 21 → 24 → 27 → 30 → 33, also +1 je Fenster und Tausch.
  - Das Muster „+1 je Tausch“ passt nur zum Taskleisten-Observer. Beide Abbau-Zeilen stehen heute in
    `v4/frame/index.js` (uncommittete Arbeit von FR6a/FR6b). Der Befund war ein Zwischenstand im Baum und lag
    **nicht am Stub**. Zwischen 68b83a3 und 74137b1 unterscheidet sich der Stub nur in Collection-Liveness,
    `reconnect` und `device_id`; keiner dieser Teile legt Observer an.
- **Die Ausnahme selbst** ließ sich nicht nachstellen. Im ersten Lauf war einmal FR4-5 rot (`toggleHidden:false`: Nach
  `sim.setPermissions([])` stand der Menüpunkt „Code ansehen“ beim Icon-Klick noch da). In zwei weiteren Läufen war
  FR4-5 grün. Das ist ein Zeitrennen im Test: `waitForFunction(… , 4000).catch(() => {})` schluckt die
  Zeitüberschreitung. Mit dem Pin hat es nichts zu tun. Es betrifft FR4/FR6a und ist nicht behoben, weil es außerhalb
  meines Auftrags liegt.
- **Abnahme-Pin:** 74137b1 ist für den Rahmen benutzbar (siehe volle Läufe).

## Befund für den Kern (Stub 74137b1)

`ctox-app-api-stub.mjs:811` sagt „je Collection im Spiegel bzw. abonniert“. `ctox-app-api-stub.mjs:817` nimmt aber nur
`SYSTEM_COLLECTIONS ∪ mirror.keys() ∪ collBacklog ∪ collError` auf. Eine App-Collection, die abonniert, aber noch leer
ist, fehlt deshalb in `metrics.collections`.

Probe: `api.query('z45_items').subscribe(…)` auf leerer Collection ergibt keinen Schlüssel `z45_items`. Erst nach dem
ersten `put` meldet der Stub `{state:'live'}`.

Folge für die Shell: `getAppState` meldet dafür `unknown` (grau, kein Badge-Eintrag), nicht `ok`. Das ist so gewollt,
weil ein fehlender Nachweis kein Grün ergibt. Der Kern sollte abonnierte Collections ab dem Abo melden, also in
`refreshCollections` auch die Namensräume aktiver Abfragen und Gets aufnehmen. Gilt dasselbe für K-1e, wäre jede
leere Collection dauerhaft grau.

## Offen bzw. Konflikte (Entscheidung durch den Orchestrator)

1. **Wächter gegen Vertrag:** `v4/tools/guards/guard-v4.mjs` meldet für `sync-state.js:33` die Regeln
   `vocab:de-verbindung` („Verbindung unterbrochen“, „Verbindung wird“) und `vocab:de-fortschritt-wird` („wird
   wiederhergestellt“), für `sync-state.js:121` die Regel `syncDiagnostics` (V3-Global). Genau diese Texte und dieser
   Rückfall stehen wörtlich im Auftrag §16.45. Die Ratchet (`v4/debt/ratchet.mjs`) war schon vorher rot (+34 insgesamt,
   größtenteils aus Parallelsträngen). ZONE45 trägt +2 de-verbindung, +1 de-fortschritt-wird und +1 syncDiagnostics
   bei. Ich habe die Regeln weder abgeschwächt noch umgangen, etwa durch einen zerlegten Namen. Nötig ist entweder
   ein Allowlist-Eintrag für `SYNC_TEXT`/V3 bis Samstag oder anderer Wortlaut. `SYNC_TEXT` ist die einzige Quelle; die
   Kopfleiste (`frame/topbar.js:57–58`) liest denselben Text.
2. **Doppelte Meldung bei Offline:** `createMetricsWatcher` (`v4/shell/app-host.js` → `status-zone.js`) meldet nach
   5 s zusätzlich „Keine Verbindung zum Server“ (`metrics.link`). Bei Offline stehen dann zwei Einträge am Badge.
   Vorschlag: In `createHostEnvironment` den Link-Teil abschalten, sobald `metrics.collections` vorhanden ist.
   `app-host.js` ist nicht in meiner Whitelist, darum nicht geändert.
3. `requested` der Shell ist heute leer, also zählen alle gemeldeten Collections. Liefert TOPBAR oder der Kern eine
   Liste der Shell-Abos, genügt `state.sync.setRequested(names)`.

## Volle Rahmen-Labor-Läufe (Endstand)

Logs liegen unter `/Volumes/tmp/z45/`.

- **Mit ZONE45, vor dem TOPBAR-Einbau** (index.js 21:23:44 durch TOPBAR geändert):
  - `frame.test.mjs mock`: **130/130** (`full-mock.log`)
  - `stub` mit Pin 74137b1: **149/149** (`full-stub-74.log`)
- **Endstand nach dem TOPBAR-Einbau (21:26):**
  - mock: 125/130
  - stub 74137b1: 144/149
  - stub 68b83a3: 139/144
  - In allen drei Läufen sind dieselben 5 Fälle rot, alle Folge der neuen Kopfleiste und nicht von ZONE45:
    - „Shell-Tausch: … ein Satz Shell-CSS“ und „Theme-Schnittstelle“: Es ist ein 5. Link `shared/shell-v2-topbar.css`
      dazugekommen, die Prüfungen erwarten 4.
    - „Doppelpuffer: Pixelprobe mitten im Tausch“: 27.55 % gleich.
    - „Doppelpuffer: genau ein Rahmen“.
    - „G0 Shell-Treue“.
  - Alle Z45-Fälle sind in allen drei Läufen grün: mock 9/9, 74137b1 8/8, 68b83a3 3/3.
- Sichtprüfung `shots/integrated-stub-z45-error-1280.png`: rotes Badge „1“ am Icon, in der Liste „Verbindung
  unterbrochen“ mit „Neu verbinden“ und „Ausblenden“. Der Zustandspunkt der Kopfleiste (TOPBAR, gespeist aus
  `currentSyncState()`) zeigt rot „Verbindung unterbrochen“. Das Fenster „Notizen“ (keine eigenen Collections) bleibt
  still.

## Runde 2 (15.10.2026, 21:27–21:47): Entscheidungen des Orchestrators umgesetzt

1. **Wächter.**
   - `tools/guards/guard-v4.mjs` hat zwei neue Ausnahmen, beide gelten für dieselbe oder die vorige Zeile:
     - `// v4-allow: zone` hebt die Vokabelregeln für Zonen-Texte auf, wie `label` umgesetzt.
     - `// v4-allow: debt L-<nr>` hebt Bezeichner-Regeln auf, wenn der Löschplan einen Eintrag hat. Ohne Nummer greift
       die Ausnahme nicht.
   - Zwei neue Tests in `guard-v4.test.mjs` (71/71). Beide prüfen den Fall mit und ohne Marker, falsche Kategorie und
     zu großen Abstand.
   - Markiert sind `sync-state.js` SYNC_TEXT (`zone`) und die V3-Standardquelle (`debt L-41`).
   - Neuer Löschplan-Eintrag **L-41** in `v4/debt/LOESCHPLAN.md`: V3-Zweig, Marker und V3-Tests werden am
     **Sa 17.10.2026** gelöscht, sobald V4-Core auf welsch aktiv ist.
   - Die Texte in den Labor-Ergebnissen erscheinen nur noch als Schlüssel (T.warn, T.error, T.action). Fallnamen ohne
     Zustandswörter.
   - **Ratchet:** Aus ZONE45 stammen keine Treffer mehr (`sync-state.js`, Z45-Fälle: 0). `de-verbindung` und
     `de-fortschritt-wird` stehen trotzdem noch je +1. Beide kommen aus `shots/results-stub.json` Z. 810, dem
     **TOP-4-Detail der Kopfleiste** (TOPBAR druckt dort das `label` des Punkts, und das ist jetzt der Zonen-Text).
     Die Datei gehört TOPBAR, darum nicht angefasst. Vorschlag an TOPBAR: im Detail einen Schlüssel statt `label`
     ausgeben.
2. **Eine Offline-Quelle.**
   - `createMetricsWatcher` (`status-zone.js`) meldet die Verbindung nicht mehr; `metrics.link` und „Keine Verbindung
     zum Server“ sind entfallen. Outbox und wachsender Rückstand bleiben.
   - Neu in `sync-state`: Bei einem Kernel ohne `collections` ergibt `metrics.link` offline den Zustand warn (Quelle
     `link`). Online allein ergibt `unknown`, nie ok.
   - `attachSyncState`: Ist nur die Verbindung weg, gilt die bisherige 5-s-Regel (`offlineDelayMs`); bei Rückstand
     bleiben es 2 s.
   - `app-host.js` musste dafür nicht geändert werden.
   - Labor-Fall Z45-10: Nach mehr als 5 s steht genau ein Eintrag je App-Badge (`shell.sync`), bei 3 s ist es still,
     nach Wiederverbindung ebenfalls. Grün in mock, 68b83a3, 74137b1 und 7b9560d.
3. **FR4-5 deterministisch.**
   - Das `.catch(() => {})` ist ersetzt durch ein echtes `waitForFunction`, das bei Zeitüberschreitung den Lauf
     scheitern lässt.
   - Ursache des Zeitrennens: Eine geschlossene Zonen-Liste behält ihre alten Kinder (alte Menüeinträge), und der
     Test hat sie mitgezählt, wenn der Icon-Klick die Liste schloss statt öffnete.
   - Jetzt wird die Liste per `zone.close()` sicher geschlossen, dann folgt der Klick, dann das Warten auf die offene
     Liste. Gezählt wird nur noch darin.
   - Volle stub-Läufe mit 74137b1 (3 parallel, 21:31–21:36): FR4-5 3/3 grün; dazu 68b83a3 grün sowie 7b9560d und
     74137b1 (21:46) grün.
4. **TOPBAR-Fälle** nicht angefasst.

Zusätzlich gefunden und behoben:
- TOPBARs TOP-4 (`sim.setBacklog(3)` ohne Collection) zeigte, dass ein **Rückstand ohne Collection-Zuordnung** den
  Gesamtzustand grün ließ. Jetzt gilt `getOverallState()`: level warn mit `backlog`, sofern `metrics.backlog` größer
  ist als die Summe der Collection-Rückstände. Der App-Zustand bleibt davon unberührt. Unit-Test ergänzt; TOP-4 ist
  seitdem grün.

**Neuer Stub-Pin 7b9560d** (Kern-Fix stub:811/817): Er ist jetzt Standard in `sync-state.test.mjs` und läuft in allen
Z45-Läufen mit.

**Neuer Fall „leere abonnierte Collection“:**
- Unit: läuft gegen jeden vorhandenen Pin.
- Labor Z45-11: Die Labor-App abonniert `z45_items` wie eine echte App.
- 7b9560d: Verlauf `unknown → warn → ok`, nach `reconnect` wieder ok, kein Badge-Eintrag.
- 74137b1 und 68b83a3: grau (`unknown`), nie grün.

**Abnahme Runde 2:**
- `node --test …/sync-state.test.mjs`: 28/28.
- `guard-v4.test.mjs`: 71/71.
- `Z45_ONLY=1`:
  - 7b9560d: 10/10
  - 74137b1: 10/10
  - 68b83a3: 5/5
  - mock: 10/10
- Volle Läufe 21:46:
  - mock: 144/144.
  - stub 7b9560d und 74137b1: je 165/167, rot sind nur `AG-0` und „Lauf ohne Ausnahme“. Ursache: Der Parallelstrang
    Agent-Seitenleiste lädt `v4/source/lab/coding-attrappe.js`, und die Datei fehlt. `index.js` wurde um 21:45 fremd
    geändert. Weil der Lauf vorher abbricht, ist der Z45-Block in diesen Läufen nicht gelaufen; abgedeckt ist er durch
    die Z45-Einzelläufe.
- Das alte `v4/lab/app-host/measure.mjs` scheitert in b und f an Klicks auf Elemente, die es seit dem
  FR6a-Badge-Umbau nicht mehr gibt bzw. die verdeckt sind (`.v4-status-zone__line` nicht klickbar, `<label>` fängt den
  Klick ab). Das passiert, bevor der Verbindungsteil erreicht wird; ZONE45 ist dafür nicht die Ursache, und
  nachgezogen ist es nicht.
