# Strang I – Rahmen (F) und App-Host (P2) zu EINER Shell zusammengeführt

Stand 15.10.2026, ca. 13:45. Klon `~/Documents/ctox-shell-v4`, Zweig `shell-v4/prototype`, kein git.
Kern-Stub: eingefrorener Stand `~/.local/state/shell-v4/stub-pins/66edb98` (per `CTOX_APP_API_STUB` überschreibbar,
in `v4/frame/lab/serve.mjs` und `v4/lab/app-host/server.mjs`).

## Ergebnis

Es gibt jetzt genau einen Einstieg und einen Chrome-Besitzer:

| Teil | Datei | Rolle |
|---|---|---|
| Einstieg | `v4/frame/index.js` | `startShell(kernel, {restored, root?, onEvent?})`, `stopShell() → snapshot`, `describeShell()`, `shellHandle()` (nur Labor/Test) |
| Fenster-Chrome | `v4/frame/index.js` + `shared/window-manager.js` | Icon, Titel, Kopf-Slot (`[data-window-actions]`, Ziel von `api.shell.registerHeader`), Steuerung, **Zonen-Platz** (`[data-v4-zone-slot]`, Gitterzeile 3 unter dem Inhaltsbereich) |
| App-Inhalt | `v4/shell/app-host.js` | `createWindowHost({root, appRecord, api, restored}) → {swap(nextVer, meta), snapshot(), destroy({keepState}), activation(ev), route(params), refocus(), ready, ver, state, zone, instance}` |
| Zustandszone | `v4/shell/status-zone.js` | rendert in den vom Rahmen gelieferten Platz; Liste im Fensterelement |
| Kernel-Brücke | `v4/shell/kernel-adapter.js` | `adaptKernel(k)` normalisiert Kern-Stub, reine Shell-Kernel-API und Labor-Attrappe |
| Stile | `v4/frame/frame.css` | Chrome **und** Zone (Tokens hell/dunkel) |

Gelöscht (doppelter Chrome-Code / Platzhalter): `v4/shell/shell.js`, `v4/shell/window-chrome.css`,
`v4/frame/host-stub.js`, `v4/frame/lab/stub-host.js`.
Einzige Änderung in `shared/`: `window-manager.js` bekommt `dispose()` (entfernt `resize` auf window/visualViewport,
trennt den Surface-ResizeObserver, je Fenster Resize-/MutationObserver, Layout-Menü-Klick auf document und
Layout-Timer; DOM bleibt stehen).

### Umgesetzte Entscheidungen

- **Rückfall:** mount-Frist 9 s (`MOUNT_TIMEOUT_MS = 9000`), danach alte Version mit demselben Snapshot. Danach
  `kernel.activation.reportFailed(appId, ver, reason)`. Der Adapter ruft die Methode nur, wenn der Kernel sie hat;
  sonst vermerkt er `gaps: ['activation.reportFailed']` und `failedReports[{delivered:false}]` (sichtbar über
  `describeShell()`). Ein Fenster versucht eine zurückgefallene Version nicht im Takt erneut (nur bei einem neuen,
  expliziten Aktivierungsereignis). Neue Fenster derselben App starten auf der letzten guten Version.
- **Doppelpuffer beim Shell-Tausch:** `stopShell()` sichert den Snapshot (auch nach `window_state` unter
  `<tabId>/__shell`), meldet alle Abos ab, ruft `wm.dispose()` und lässt den alten Rahmen eingefroren stehen
  (`data-v4-retiring`, keine Zeigerereignisse). `startShell()` baut den neuen Rahmen mit `opacity: 0` darüber
  (opacity statt visibility, damit Fokus/Auswahl/Scroll schon beim Montieren gesetzt werden können). Der neue Rahmen
  wartet auf alle `host.ready` (höchstens 9,5 s) und schaltet dann **synchron in einem Schritt** um: Ereignis
  `v4:retire` an den alten Rahmen (der entfernt sein Root und baut danach seine Apps mit `keepState` ab), altes
  Shell-CSS weg, neues Root sichtbar, `host.refocus()`. Das gilt auch über Modulgrenzen (shell@1 → shell@2 sind
  verschiedene Modul-Instanzen; Vertrag nur über DOM-Attribut und Ereignis).
- **Fensterzustand:** `kernel.windowState.get/put/del`, wenn vorhanden (Attrappe), sonst IDB-Ersatz im Adapter
  (`ctox-shell-window-state-<instanz>-<nutzer>`, gemeldet als `gaps: ['windowState']`, gilt für den Pin 66edb98).
- **Titel:** Fenstertitel = Titel der App (Manifest/`kernel.apps`), nie die Version. Die Labor-Apps setzen
  `setTitle('Formular')` statt `'Formular · v1'`.
- **Kernel-Abos ohne Leck:** Der Adapter abonniert den Kernel nur durch Weiterreichen (kein eigenes Dauer-Abo);
  nach `stopShell()` hält eine Shell keine Abos mehr.

## Abnahme (selbst ausgeführt, alle grün)

| Befehl | Ergebnis |
|---|---|
| `node src/apps/business-os/v4/frame/lab/frame.test.mjs mock` | **37/37 bestanden** |
| `node src/apps/business-os/v4/frame/lab/frame.test.mjs stub` (Pin 66edb98, `sim.swapShell`) | **33/33 bestanden** |
| `node src/apps/business-os/v4/lab/app-host/measure.mjs` (Szenarien a, b, c, c2, d, e, f, h, g) | **alle bestanden**, Exit 0 |
| `node src/apps/business-os/v4/lab/app-host/measure-stub.mjs --reps 6` (Zusatz, nicht gefordert) | a-stub und c-stub bestanden |

Neue Tests in `frame.test.mjs` (mock und stub):
- **Shell-Tausch mit 3 Fenstern ohne leeren Frame.** Die Lab-Apps montieren dafür künstlich 600 ms lang
  (`globalThis.__labMountDelayMs`). Während des Tauschs stehen 2 Rahmen (neu `data-v4-building`, alt
  `data-v4-retiring`). MutationObserver über den ganzen Shell-Baum plus rAF-Abtastung: Es sind nie weniger als
  3 sichtbare Fenster mit App-Inhalt (mock: 20 Mutationsproben / 91 Frames, jeweils min 3; stub: 17 / 90, min 3).
  Pixelprobe: Der Screenshot 250 ms in den Tausch hinein stimmt zu **100,00 %** mit dem Bild vorher überein
  (`shots/integrated-swap-mid-1280.png`).
- **Listener-Zahl nach 5 Shell-Tauschen konstant.** Gezählt werden globale Listener (window/document/visualViewport),
  lebende Resize-/MutationObserver, Kernel-Abos, Rahmen und Fenster. mock: `19/8/2/1/3` in allen 5 Läufen, stub:
  `21/8/–/1/3`. **Gegenprobe ohne `wm.dispose()`:** `34/29 → 39/36 → 44/43 → 49/50 → 54/57` (+5 Listener,
  +7 Observer je Tausch); der Test schlägt dann fehl.
- Fenstertitel ohne Versionsnummer. Zonen-Platz im Chrome, ohne Einträge 0 px. Zone mit Eintrag: 23 px, unten im
  Fenster unter dem Inhaltsbereich. Formularwert nach 6 Tauschen erhalten.

### Messwerte measure.mjs (Endlauf 13:42; Load-Average 47–59, fremde Last)

| Szenario | Wert |
|---|---|
| (a) Veröffentlichen → mounted, 20 Remounts | B p50 5,4 / p95 7,3 ms, A p50 3,9 / p95 5,8 ms, 0 Abweichungen |
| (b) generische Sicherung | 11,9 ms, 0 Abweichungen |
| (c) mount hängt → Rückfall | **9014 ms** (A und B), ≤ 10 s; `reportFailed` 2× (je Tab), Zeiger der Attrappe danach auf 2 (zurückgenommen); Zone im Chrome; v2 erneut veröffentlicht → Zone leer |
| (c2) mount wirft → Rückfall | 75 ms |
| (d) Live-Daten Tab A → Tab B | p50 2,2 / p95 2,8 ms |
| (e) Shell-Tausch 1→2 (Doppelpuffer), 3 Fenster | stop 1,6 + import 13 + start/umschalten 19,8 = 34,4 ms; 0 Abweichungen; nie leer (min 3 über 13 Mutationen / 6 Frames) |
| (f) Zustandszone | leer 0 px; 1 Zeile 23 px; Liste im Fenster; 2-s-Regel und Messwert-Schwellen wie P2 |
| (h) 10 Fenster, 9 gleichzeitig remountet | p50 13,7 / p95 24,8 ms; CSS neu geladen vor alt entfernt |
| (g) kein Reload, kein Datenabruf | 68 Anfragen (2 document, 53 script, 13 stylesheet), 0 unerwartet, 0 Konsolenfehler |
| (c-stub) Pin 66edb98 | Rückfall nach 9015 ms; **Stub-Zeiger bleibt auf 99** (keine `reportFailed`-Methode → Lücke vermerkt) |

## Screenshots (angesehen)

`v4/frame/lab/shots/integrated-frame-1280.png`, `integrated-frame-900.png`, `integrated-frame-420.png`,
`integrated-frame-420-launcher.png`, `integrated-frame-1280-desktop.png`, `integrated-zone-1280.png`,
`integrated-swap-mid-1280.png`, dazu dieselben mit `integrated-stub-`. Aus dem Host-Labor:
`v4/lab/app-host/shots/{a,c,e,f,h}-*.png` (jetzt im Rahmen-Chrome).
Behobener Layoutfehler: Ohne Icon zeigte das Fenster nur einen nackten Buchstaben, Launcher und Taskleiste dagegen
eine getönte Kachel. Jetzt ist es dieselbe getönte 22-px-Kachel. Geprüft und unauffällig: Zone als eine Zeile unten im Fenster,
Liste im Fenster über der Zone, Kopf-Slot mit App-Knopf („Speichern“/„Neu“) links der Steuerung, 420 px
vollflächig ohne Überlauf. In den 1280er-Bildern fehlen bei hinteren Fenstern Icon und Titel. Das ist Überdeckung durch das
vordere Fenster, kein Renderfehler (per DOM geprüft: sichtbar, volle Breite).

## Offene Punkte / Lücken

1. **Kern-Stub ohne `activation.reportFailed`** (Pin 66edb98): Der Zeiger bleibt nach Rückfall auf der kaputten
   Version. Die Shell schützt sich selbst (kein Wiederholungs-Takt, neue Fenster auf letzter guter Version), aber
   andere Tabs/Geräte erfahren es nicht. Gebraucht: `activation.reportFailed` im Kernel.
2. **Kein `windowState` in der Shell-Kernel-API** des Stubs: Der IDB-Ersatz im Adapter ist aktiv.
3. Während des Doppelpuffers (Dauer = langsamstes `mount`, höchstens 9,5 s) ist der alte Rahmen eingefroren:
   Eingaben nach `stopShell()` landen in der alten Instanz und gehen verloren. Gemessen dauert das 20–60 ms.
4. Bricht der Loader nach `stopShell()` ab, ohne eine Shell zu starten, bleibt der eingefrorene alte Rahmen
   stehen. Der Rückfall des Loaders auf die alte Shell räumt ihn mit auf.
5. `registerHeader` nimmt nur `Node` an (Stub-Typ: `spec: unknown`). Eine Entscheidung im Vertrag steht aus.
6. Im Labor stammen beide Shell-Versionen (`/~a/shell@1/…`, `/~a/shell@2/…`) aus demselben Quellstand. Sie sind
   aber getrennte Modul-Instanzen mit eigener CSS-Registry. Ein inhaltlich anderes Shell-Release ist nicht gemessen.
7. **Abhängigkeit eines anderen Strangs:** `v4/source/source-mode.css` (Quelltext-Modus) liest die `--v4-*`-Tokens
   der gelöschten `shell/window-chrome.css`. `frame.css` definiert sie deshalb als Alias auf die `--v4f-*`-Tokens.
   Sie gelten innerhalb des Rahmen-Roots `.v4f`, nicht auf `:root`. Der Quelltext-Modus muss also im Rahmen gerendert werden.
   Die Kommentare in `v4/source/*.css`, die auf `window-chrome.css` zeigen, liegen außerhalb meines Auftrags und sind
   nicht geändert.

---

## Nachtrag Strang PIN: Kern-Stub 66edb98 → 67229d2 (§16.19), 15.10.2026 ca. 14:30

Die Shell und beide Labore laufen jetzt gegen `~/.local/state/shell-v4/stub-pins/67229d2`. Das ist der Standard in
`v4/frame/lab/serve.mjs` und `v4/lab/app-host/server.mjs`, `CTOX_APP_API_STUB` überschreibt ihn weiter.
Die Lücken 1, 2 und 5 von oben sind geschlossen.

### Was sich geändert hat

| Kernel-Funktion (§16.19) | Nutzung in der Shell |
|---|---|
| Nr. 2 `kernel.activation.reportFailed` | `app-host.js` meldet jeden Rückfall und jeden fehlgeschlagenen Erst-Mount. Der Adapter wartet auf das Ergebnis und protokolliert `failedReports[{delivered, activated_ver}]`. Danach steht der Zeiger in allen Tabs auf der letzten guten Version, und `kernel.apps.activated_ver` folgt. Neue Fenster starten deshalb auf der guten Version. `brokenVer` im Rahmen bleibt nur als Schutz für Kernel ohne die Methode (Attrappe). Die Kernel-Meldung `app.activation_failed` wird in einem Fenster ausgeblendet, das selbst zurückgefallen ist, denn dort steht schon „… läuft weiter“. |
| Nr. 3 `kernel.windowState.get/put/delete` | Der IDB-Ersatz im Adapter ist entfernt. Gespeichert wird je Fenster unter `mount:<fenster>` und für die Shell unter `shell`. Tab und Nutzer trennt der Kernel. Nur die Labor-Attrappe ohne Tabs bekommt `<tabId>/` vorangestellt. Ohne Kernel-`windowState` bleibt eine reine Speicher-Map und die Lücke `windowState`. Neu: Bekommt `startShell` keinen Snapshot vom Loader, liest es `windowState.get('shell')`. Damit kommt die Fensteranordnung beim Neuladen desselben Tabs zurück. **Nicht im Test.** |
| Nr. 3 `shell_preferences` | Pins und Launcher-Reihenfolge liegen in genau einer Zeile `{id: actor.id, taskbar_pins: {version, pins, updated_at_ms}, launcher_order, updated_at_ms}`. Geschrieben wird über `kernel.appApi('shell', <shellVer>).put`, gelesen live über `get(...).subscribe`. Ändert eine andere Shell die Zeile, übernimmt der Rahmen sie, wenn sie jünger ist. Der Shell-Snapshot trägt dieselben Werte, es gilt der jüngere Stand. Die Rahmen-Attrappe ohne App-API nutzt `prefs.get/set('shell_preferences')`. |
| Nr. 6 `kernel.apps` mit `layout`/`styles` | Der Launcher zeigt echte Icons: `layout.icon_asset` als versionierte URL, `layout.icon_svg` als `data:image/svg+xml`. Die Gruppen kommen aus `layout.category`, Monogramme nur noch ohne Icon. Neue Fenster nehmen `default_width/height` und `min_width/min_height` aus dem Layout, `frame_palette` steht als `data-frame-palette` am Fenster. Der Host lädt App-CSS aus `manifest.styles` in dieser Reihenfolge. Nur wenn das Feld fehlt (Attrappe), lädt er die `*.css`-Dateien. Das Manifest wird nur noch gelesen, wenn ein Eintrag kein Layout trägt. |
| Nr. 1 `kernel.status` (Strom) | `statusZone` ersetzt. `api.status.report/clear` der App geht an den Kernel und kommt über `kernel.status.forApp(app)` zurück, mit den Quellen `app` und `kernel`. Rückfall-, Mount- und Snapshot-Einträge des Hosts bleiben fensterlokal. Ohne Strom (Attrappen) bleibt der direkte Weg. |
| Nr. 7 `link.state ∈ online \| offline` | Der Adapter bildet die Metriken darauf ab (Attrappe `up/down` → `online/offline`). `status-zone.js` prüft nur noch auf `online`. |
| Nr. 4 Fristen 9 s / 10 s | mount-Frist 9 s, Rückfall gemessen 9012–9025 ms. Der Doppelpuffer beim Shell-Tausch wartet jetzt höchstens **8,5 s** statt 9,5 s. Grund: Die Startfrist des Loaders (`shellStartTimeoutMs`) beträgt 9 s. Mit 9,5 s wäre der Loader mitten im Umschalten auf die alte Shell zurückgefallen. |
| Nr. 5 `registerHeader(el: HTMLElement \| null): void` | Ein Element kommt per Referenz in den Kopf-Slot. Ein neues Element ersetzt das alte, `null` entfernt es, alles andere wirft `CtoxError invalid`. Die Referenz geht zusätzlich an den Kernel (`kernel.shell.window(app).state.header`). Die Labor-Apps `formular` v1/v2 rufen beim Abbau `registerHeader(null)` auf, statt eine Rückgabe-Funktion zu nutzen. |
| entfallene Stub-APIs | `stub-lab.html` veröffentlicht ohne `module` und mit `layout`. Die Apps montiert der eigene Host (`v4/shell/app-host.js`), Code kommt über `/~a/`. `kernel.shell.mount` und `sim.registerModule` nutzte keines der Labore. `measure-stub.mjs` veröffentlicht mit `styles`. `testkit/app-host.mjs` wird nicht gebraucht, weil der eigene Host diese Rolle hat. |

### Abnahme (selbst ausgeführt, `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/67229d2`, alle grün)

| Befehl | Ergebnis |
|---|---|
| `node src/apps/business-os/v4/frame/lab/frame.test.mjs mock` | **40/40** |
| `node src/apps/business-os/v4/frame/lab/frame.test.mjs stub` | **47/47** |
| `node src/apps/business-os/v4/lab/app-host/measure.mjs` | alle 9 Szenarien bestanden, Exit 0 (c: Rückfall 9012 ms, Zeiger der Attrappe zurück) |
| `node src/apps/business-os/v4/lab/app-host/measure-stub.mjs` (20 Wdh.) | a-stub: B p50 13,9 / p95 25,5 ms, 0 Abweichungen. c-stub: Rückfall nach **9025 ms**. `reportFailed` 2× zugestellt (Tab A und B). Zeiger danach in beiden Tabs `{activated_ver: 21, target_ver: 99, state: failed, error: 'reported: mount() nicht binnen 9 s fertig'}`, `kernel.apps.activated_ver = 21`. **Neues Fenster in A und B startet 21 (mounted)**. |

Neue Prüfungen in `frame.test.mjs`:
- **stub:** Launcher-Icons aus `layout.icon_asset` als `/~a/notes@1.0.0/icon.svg`, geladen mit `naturalWidth > 0`. Gruppen `Arbeit,Finanzen`, keine Monogramme. `layout.icon_svg` (crm) als data-URL. Fenstergröße aus `default_width/height` (700×480). Das Launcher-Icon folgt der Version 1.1.0. Ein Zustandseintrag der App läuft über `kernel.status` (`source: 'app'`).
- **beide:** `registerHeader` mit Referenz, Ersetzen, `null`, `CtoxError invalid` und Rückgabe `undefined`. Die Pin-Zeile steht in `shell_preferences` mit `id = owner`.
- **stub, neuer Test:** „Pins überleben Shell-Tausch und neuen Tab über shell_preferences“. Ein Browser-Kontext, IndexedDB, Mehr-Tab, eigene Instanz je Lauf. Tab 1 heftet `tasks` und `ledger` an und setzt die Launcher-Reihenfolge `tasks,notes`, dann Shell-Tausch: beide Werte bleiben. Tab 2 ist neu (Leser, neuer `tabKey`, `windowState.get('shell') === null`). Er zeigt dieselben Pins und dieselbe Reihenfolge, also allein aus `shell_preferences`. Löst Tab 2 einen Pin, verschwindet er **live** in Tab 1. Keine Seitenfehler.

Screenshots angesehen: `v4/frame/lab/shots/integrated-stub-frame-1280.png` und `-1280-desktop.png` zeigen echte Kachel-Icons (N/B/K aus `icon.svg` bzw. `icon_svg`) unter den Gruppen ARBEIT/FINANZEN/VERTRIEB. `integrated-stub-frame-420.png` und `-420-launcher.png` laufen vollflächig ohne Überlauf, die Taskleiste zeigt nur Icons. `v4/lab/app-host/shots/c-stub-rueckfall-neues-fenster-tab-b.png` zeigt links das zurückgefallene Fenster („Version 99 startet nicht …; 21 läuft weiter“, Eingaben erhalten, Knopf „Neu“ über `registerHeader` im Kopf-Slot) und rechts das neue Fenster auf 21.

### Offen

1. Für die Launcher-Reihenfolge gibt es Speicher, Live-Abgleich und Sortierung (`launcher.setOrder`), aber **keine Bedienung** (Ziehen der Kacheln). Gesetzt wird sie im Test über `shellHandle().setLauncherOrder(ids)`.
2. Das Wiederherstellen aus `windowState.get('shell')` beim Neuladen desselben Tabs ist eingebaut, aber nicht getestet. Die Labore laden nie neu.
3. Ein neues Fenster auf der guten Version zeigt die Kernel-Meldung „Aktivierung formular@99 fehlgeschlagen …“ in seiner Zone (Quelle `kernel`). Das ist gewollt, weil das Fenster den Fehlschlag nicht selbst erlebt hat. Wann der Kernel den Eintrag räumt, entscheidet der Kernel.
4. `frame_palette` wird nur als Attribut gesetzt. `frame.css` wertet es noch nicht aus.
5. App-Einträge mit `source: 'app'` gelten im Kernel je App, nicht je Fenster. Zwei Fenster derselben App im selben Tab zeigen deshalb beide den Eintrag.

---

## Nachtrag Strang FR2: Aufgaben der App-Ports an den Rahmen, 15.10.2026

Kern-Stub `~/.local/state/shell-v4/stub-pins/cff5ba5`. Geändert nur unter `v4/frame/` und `v4/shell/`; `shared/resizer.js`
unverändert (wird nur importiert). Keine `modules/**`. Grundsatz (Owner): der Rahmen hat nur Standardschnittstellen,
keine App-IDs und keine App-Sonderfälle.

### Umgesetzt

| Nr. | Was | Wo |
|---|---|---|
| 1 | Inhaltsbereich jedes Fensters ist Size-Container `business-app-window` (`container: business-app-window / inline-size` an `.shell-window-content`). Haltepunkte 1024/768 der Apps greifen ohne eigene Deklaration. Eine zusätzliche Deklaration am App-root (placements, intake, … 21 Apps) schadet nicht: gemessen dieselben Haltepunkte. | `frame.css` |
| 2 | Im Inhaltsbereich `--shell-v2-icon-size: 0px`, `--shell-v2-icon-frame-span: 0px`, `--shell-v2-close-size: 0px`, neu `--shell-v2-control-inset: 0px`. Am Fenster-Chrome bleibt `--shell-v2-icon-size: 28px` (window-manager liest es dort). Linke Zone weg: knowledge-Kopf 36 → 8 px, documents 42 → 14 px, crm-Labor 94 → 14 px. | `frame.css` |
| 3 | `v4/shell/column-resizers.js`: verdrahtet jeden `.ctox-column-resizer`/`.ctox-row-resizer[data-resizer-var]` im Inhaltsbereich mit `CtoxResizer` (Zeiger + Tastatur), nach jedem mount/Remount, und Griffe, die die App später rendert (MutationObserver, nur childList). Breiten in `kernel.windowState` unter `cols:<app>:<fenster>` = `{app, widths}`; zusätzlich `cols:<app>` als Startwert für neue Fenster derselben App. Schließen löscht den Fensterschlüssel, Shell-Tausch behält ihn. Gegen echte Apps geprüft: knowledge, placements, intake, support, tickets (Zug ändert die Variable, Wert steht in window_state). | `shell/column-resizers.js`, `shell/app-host.js` |
| 4 | Mehrfenster: jede Instanz hat eigenes root, eigenes `api`-Objekt (geprüft im Test). Neu: `layout.presentation.multi_instance === false` verhindert ein zweites Fenster (vorhandenes nach vorn). Liste der Apps mit modulweitem Zustand unten. | `index.js` |
| 5 | desktop-Abgaben, siehe Tabelle unten. Neu: Kachel auf die Taskleiste ziehen = anheften (an Position vor dem Pin unter dem Zeiger), Pin in der Leiste umordnen, Pin auf die Fläche ziehen = lösen. | `launcher.js`, `taskbar.js`, `index.js` |
| 6 | Launcher-Reihenfolge per Ziehen (innerhalb der Gruppe, Einfügemarke links/rechts) und per Tastatur Alt+←/→; Speicherung über `setLauncherOrder` in `shell_preferences`. | `launcher.js`, `frame.css` |
| 7 | `api.shell.navigate(app, params)` reicht die Parameter generisch durch, als **RouteEvent `{from, to, params}`** (Stub-Vertrag `index.d.ts`). **Fehler behoben:** der Host rief `onRoute` bisher mit den nackten params auf, alle portierten Apps lesen aber `ev.params` (ctox, desktop, documents, browser …), Deep-Links kamen also nie an. Ziel geschlossen: Fenster geht auf, die Route geht als `ctx.route` an `mount(root, api, {restored, route})` UND genau einmal als erstes `onRoute`-Ereignis (dasselbe Objekt), auch wenn die App `onRoute` erst nach mount abonniert (browser OFFEN-9). Kommt eine Route vor dem ersten Abonnenten, wird sie vorgemerkt statt verworfen. | `shell/app-host.js`, `index.js` |
| Z1 | Zustandszone: Eintrag mit `action {label, run}` zeigt einen knappen Knopf am Eintrag (Zeile und Liste). Klick ruft `run()`; ohne Fehler (und nicht `false`) verschwindet der Eintrag (über den Kernel-Strom: `api.status.clear`), sonst bleibt er. Funktionen laufen nicht über den Kernel-Strom; der Host hält sie je Code. | `shell/status-zone.js`, `shell/app-host.js`, `frame.css` |
| Z2 | Theme als Standardschnittstelle: der Rahmen lädt `theme/theme.css` (generiert von `theme/build-theme.mjs`: nur die Tokens, die Apps nachweislich nutzen, 29 hell / 24 dunkel aus app.css + ui-contract, base.css-eigene nicht doppelt), `../../shared/base.css` (relativ zum Modul, also im Artefakt `/~a/shell@<v>/shared/base.css`) und `frame.css`. Hell/Dunkel über `:root[data-theme]` wie app.css; ohne Vorgabe des Hosts setzt der Rahmen es aus `prefers-color-scheme` und folgt Änderungen. `frame.css`-Dunkelwerte hängen jetzt an `:root[data-theme="dark"]` statt an der Media-Query. Prüfen: `node v4/frame/theme/build-theme.mjs --check`. GEO2 (15.10.): theme.css trägt zusätzlich die generischen Kit-Regeln der Pane-Grammatik aus app.css (`.ctox-pane-header/-title-row/-titles/-kicker/-title`, `.drawer-body`, `.drawer-header-row`, `.os-scrollbar`; Klassenliste `KIT_CLASSES` im Generator, jede Klasse muss von mindestens einer App genutzt sein), jetzt 30 hell / 24 dunkel + 15 Kit-Regeln. Ohne `.ctox-pane-title-row` rutschten die Kopf-Icons unter den Titel. | `index.js`, `theme/`, `frame.css` |
| Z3 | `data-app` sitzt nur noch am Inhalts-root; das Fensterelement trägt `data-window-app`. `[data-app="x"] button` trifft keine Fensterknöpfe mehr. | `index.js` |
| Z4 | Keine App-Sonderfälle: kein Abhören von `ctox-business-os-open-chat`/`ctox-crew-workload` im Rahmen (zwischenzeitlich gebaut, wieder entfernt). | `index.js` |

### FR4 (15.10.): Spaltengrammatik, Zonenregel, navigate-Ziele, gesperrte Knöpfe

**Zonenregel (verbindlich, `shell/status-zone.js`):**

| Stufe | Lebensdauer |
|---|---|
| `success` | wird nie angezeigt (Erfolg ist still; der Kern-Stub lehnt `success` an `api.status` ohnehin als `invalid` ab) |
| `info` | verschwindet nach **6 s** von selbst (`INFO_TTL_MS`). Beim Ablauf räumt die Zone über `dismiss` auch den Kernel-Eintrag der App (`api.status.clear`). Liefert der Kernel-Strom denselben Inhalt (Stufe + Text + ref) erneut, bleibt er weg; neuer Text oder `clear(code)` heben das auf. |
| `warn`, `error` | bleiben, bis die Quelle sie räumt (`clear`) oder sie quittiert werden. Mit eigener Aktion (`action {label, run}`, Z1) quittiert deren Erfolg. **Ohne eigene Aktion** bekommt jeder Eintrag generisch die Zonen-Aktion **„Ausblenden“** (z. B. Befehlsfehler `command:<id>` aus `commandLifecycle`, `shell.navigate`). |

Ausblenden gilt **je Tab** (`sessionStorage`, Schlüssel `ctox-v4-zone-ack:<app>`, Bereich aus `data-window-app` des
Fensters): derselbe Eintrag (code + Stufe + Text + ref) bleibt ausgeblendet, auch wenn der Kernel-Strom ihn erneut liefert
oder die Shell getauscht wird. Der Kernel-Eintrag bleibt unberührt (andere Tabs sehen ihn). Räumt die Quelle den Code,
verfällt die Quittung; ein späteres Wiederauftreten erscheint wieder. Ein späteres `done` auf demselben Befehlsziel räumt
einen Befehlsfehler absichtlich nicht ab.

| Nr. | Was | Dateien |
|---|---|---|
| FR4-1 | Spaltengrammatik (`data-pg-*`: Suche, Filterablage, Reset, Ansicht, Bänder, Zähler) verdrahtet der Rahmen im Inhaltsroot – nach jedem mount/Remount und für Panes, die die App später rendert (gleicher Lebenszyklus wie FR2-3, `autoWirePaneGrammar` aus `shared/pane-grammar.js`). Idempotent über die DOM-Marke `data-pg-wired`: Apps, die selbst verdrahten (zurzeit ~45 Apps, u. a. appsec-pentest, coding-agents), werden erkannt und nicht doppelt verdrahtet (gleicher Griff `pane.__ctoxPaneGrammar`, ein Ereignis je Eingabe, geprüft mit eigener Modulinstanz). Die App-Aufrufe bleiben bewusst: viele Apps lesen den Griff noch während `mount` (`setCounts`/`setFooter`), und ihre v4-api-Tests montieren ohne Rahmen. Neue Apps brauchen den Aufruf nicht (z. B. outbound, RUND2). | `shell/column-resizers.js` |
| FR4-2 | Zonenregel oben. | `shell/status-zone.js`, `shell/app-host.js` (dismiss je App-Eintrag) |
| FR4-3 | `navigate` auf ein Ziel ohne v4-App liefert `false` und genau einen Eintrag `shell.navigate` („Ziel „x“ ist nicht vorhanden“, error, „Ausblenden“) in der Zone der aufrufenden App. Aufrufer ohne Ziel: `modules/coding-agents/index.js:390` `navigate('code-editor', {moduleId, moduleTitle})` – code-editor ist seit UC4 eine **Bibliothek** (`desktop-apps/code-editor/app.js`, kein Manifest in `v4/manifests/`), also kein Navigationsziel; die App muss den Editor selbst einbetten (`createCodeEditor`) oder den Knopf entfernen → coding-agents-Strang. `modules/ctox/index.js:2425` `navigate('shell', {open:'settings', tab:'channels'})` – der v4-Rahmen hat keine Einstellungsfläche (Kernel-/Vertragswunsch 3) → ctox-Strang bzw. Entscheidung über Shell-Ziele. | `shell/app-host.js` (bestand) |
| FR4-5 | Strang NAV (15.10.) löst beide FR4-3-Aufrufer: **`api.shell.openSource(appId?) → boolean`** (neu, generisch, ersetzt v3 `ctx.openModuleSource`) öffnet die Quelltext-Ansicht einer App (Standard: die eigene) als Fenster des Rahmens (`data-window-view="source"`, `data-window-source=<app>`, Titel „Quelltext – <App>“, `openSourceMode` aus `v4/source/source-mode.js`). Nur mit `apps.source.view` (Spiegel; ohne `apps.source.edit` nur lesbar) und `kernel.source`; eine Ansicht je App (zweiter Aufruf fokussiert), übersteht den Shell-Tausch (Snapshot `app_id: shell:source:<app>`), schließt bei Rechteentzug oder wenn die App verschwindet. Nicht möglich → `false` + genau ein Eintrag `shell.openSource` („Quelltext von „x“ ist nicht verfügbar“, error, „Ausblenden“) in der Zone der aufrufenden App. coding-agents ruft `openSource(mod.id)` statt `navigate('code-editor')`. ctox: Knopf „Kanäle verwalten“ entfernt – keine v4-App trägt die Kanal-Einrichtung (communication_accounts lesen nur mail/conversations/ctox, keine richtet Konten ein; die frühere Shell-Einstellung `channels` aus `shared/react-settings.js` hat kein v4-Gegenstück) → Kernel-/Vertragswunsch 3 bzw. eigene Kanal-App. | `frame/index.js`, `shell/app-host.js`, `tools/port/app-api.d.ts` |
| FR4-4 | Gesperrte Kit-Knöpfe (`.ctox-button`, `.ctox-icon-button`, `.ctox-run-control` `:disabled`): neutrale Fläche `--kit-fill`, Rand `--line`, Schrift `--muted`, volle Deckkraft statt `opacity .5/.55`; Ghost ohne Fläche. Gemessen (Text gegen sichtbaren Knopfgrund, im Fenster): hell 4,41:1, dunkel 3,28:1 für normal, is-primary, run-control, Icon, is-danger (vorher hell bis 1,33:1 bei run-control, 2,63:1 bei is-primary). | `theme/build-theme.mjs` → `theme/theme.css` |

Prüfung: `frame.test.mjs mock|stub`, Abschnitt FR4 (7 Prüfungen; FR4-5: mock 1, stub 3). Gegenproben rot: ohne `autoWirePaneGrammar` im Scan
(FR4-1 3/3 rot), ohne Ablauf (`INFO_TTL_MS` 600 s: FR4-2 info rot), ohne Quittung (FR4-2 Ausblenden rot), ohne Kit-Regel
(FR4-4 rot, 1,33:1).

### Grenze bei Aufgabe 2: rechte Steuerzone

Eine Variable für die rechte Zone gibt es in den Apps nicht. Sie rechnen `padding-right: calc(var(--shell-v2-header-row-size, 37px) * 2)`
(73 Stellen in 25 index.css), teils fest `70px` (ctox, mail `--mail-head-inset-right`) oder app-lokal (`--tickets-v2-control-inset`,
`--invoices-v2-control-inset`). `--shell-v2-header-row-size` ist zugleich die Kopfband-HÖHE (knowledge rechnet mit dem Rückfall 61px);
auf 0 gesetzt fallen die Köpfe zusammen, auf 37px gesetzt drängt knowledge seine Kopfaktionen über die Filterzeile (beides
gemessen und verworfen). Der Rahmen setzt deshalb nur die neue `--shell-v2-control-inset: 0px`.
**PORTING R18 (Vorschlag für den Port-Strang):** `calc(var(--shell-v2-header-row-size, 37px) * 2)` als rechte Reserve
ersetzen durch `var(--shell-v2-control-inset, calc(var(--shell-v2-header-row-size, 37px) * 2))`, feste 70px ebenso.
Dann verschwinden die 74 px rechts ohne weitere Rahmenänderung.

### FR2-4: Apps mit modulweitem Zustand (aus den V4-PORT.md, keine App-Datei geändert)

Zwei gleichzeitige Montagen teilen Zustand (die zweite übernimmt die Darstellung): knowledge (OFFEN-4), calendar (3),
appsec-pentest (2), buchhaltung (4, dazu 28 `window.*`-Handler, 58 `document.*`-Zugriffe), creator (4), coding-agents (2),
customers (5), invoices (4), iot (4), kundenpipeline (3), matching (1, `multi_instance:false`), news (4), outbound (8),
notes (5), reports (3), tickets (4), shiftflow (1, erste Montage danach ohne API), support (3), threads (7,
`multi_instance:false`). Nur Texte/Locale modulweit: esign (1), consent (1), intake, browser (8, `multi_instance:false`).
Zustand je Montage: ctox, documents (11, `multi_instance:false`). Ohne Angabe: übrige Apps.
Der Rahmen öffnet je App normal genau ein Fenster (Launcher, Taskleiste, navigate fokussieren das vorhandene). Ein zweites
Fenster entsteht nur über `shellHandle().openWindow(…, {multi})` oder Wiederherstellung und wird bei
`presentation.multi_instance === false` verweigert. Empfehlung an den Manifest-Strang: `multi_instance:false` für alle
oben genannten Apps mit modulweitem Zustand setzen, bis sie Zustand je Instanz haben.

### FR2-5: was desktop an den Rahmen abgegeben hat

| desktop V4-PORT | Rahmen |
|---|---|
| 1 Anheften an Taskleiste | Pin-Knopf an der Kachel (bestand), neu Ziehen Kachel → Leiste, Pin umordnen, Pin → Fläche löst. Pins in `shell_preferences.taskbar_pins` (nicht mehr `desktop_layout.taskbar_pins`; desktop legt dort weiter Standards an, die der Rahmen nicht liest → Manifest-/desktop-Strang entscheidet, ob das Feld entfällt). |
| 2 Launcher-Katalog | Launcher aus `kernel.apps` (bestand). |
| 3 „CTOX live“-Toast je Befehl | **entfällt bewusst** (Owner-Regel: keine Pillen/Banner/Toasts). Fehler eines Befehls meldet die sendende App in ihre Zone. |
| 4 Sync-Anzeige, Puls | Messwerte-Wächter der Zone (`createMetricsWatcher`, bestand): nur auffällige Zustände (Verbindung > 5 s weg, Outbox > 10 s, Rückstand wächst). Dauer-Anzeige entfällt bewusst. |
| 5 Erfolgsmeldungen | entfallen bewusst; Fehler über die Zone der App. |
| 6 Kontext-Chat | `navigate('ctox', {chat})` wird generisch durchgereicht (Aufgabe 7). Die CTOX-App liest `params.chat` noch nicht (`normalizeFocusTask` kennt nur task_id/command_id) → ctox-Strang. |
| 7 Icon-Glyphen | keine Icon-API (OFFEN-D), unverändert. |

### Kernel-/Vertragswünsche (offen, nicht im Rahmen gelöst)

1. **Chat öffnen** (ctox OFFEN-3, support OFFEN-2): es gibt in v4 keine Chatleiste. Wunsch `kernel.chat.open({command_id, task_id, source})`
   oder Entscheidung, dass die CTOX-App das Chat-Ziel ist (dann reicht `navigate('ctox', {chat})`). Die ctox-App
   feuert heute `ctox-business-os-open-chat` als DOM-Ereignis; der Rahmen hört es absichtlich nicht (kein App-Sonderfall).
2. **Crew-Auslastung** (ctox `window.__ctoxCrewWorkload`/`ctox-crew-workload`): Wunsch als Kernel-LiveDoc
   (`kernel.crew.workload`), geschrieben über die normale App-API; eine Crew-Leiste im Rahmen gibt es noch nicht.
3. **`navigate('shell', {open:'settings', tab})`** (ctox OFFEN-4): der Rahmen hat keine Einstellungsfläche; `shell` ist
   kein App-Ziel → `false` + Zoneneintrag. Braucht eine Entscheidung, welche Shell-Ziele es gibt.
4. **Rückmeldung an den öffnenden Thread** (`ctox-business-os-record-focus`, documents OFFEN-6, tickets OFFEN-2): kein Weg in 4.0.
5. Stub-Vertrag: `AppMount`-Kontext um `route?: RouteEvent` ergänzen (der Host liefert ihn jetzt).

### Befunde außerhalb des Auftrags

- **documents startet im Rahmen nicht zuverlässig**: `modules/documents/index.css:1` `@import url('ctox:shared/office-workspace.css')`.
  `ctox:` ist für den Browser ein unbekanntes Schema (`ERR_UNKNOWN_URL_SCHEME`), ein Service Worker kann es nicht abfangen.
  Fällt das `<link>` damit auf `error`, scheitert der Erst-Mount („CSS … nicht ladbar“), `reportFailed` nimmt die App aus
  `kernel.apps`. Im Geometrie-Lauf mal so, mal so (zwei Läufe ok, zwei Läufe weg). Gehört dem documents-/SW-Strang
  (CSS-Importe relativ oder über `styles` im Manifest).
- `v4/lab/app-host/measure.mjs` nicht gelaufen (schreibt nach `v4/lab/app-host/shots`, außerhalb meines Schreibbereichs).

### Abnahme (selbst ausgeführt)

| Befehl | Ergebnis |
|---|---|
| `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/cff5ba5 node v4/frame/lab/frame.test.mjs mock` | **59/59** |
| `CTOX_APP_API_STUB=… node v4/frame/lab/frame.test.mjs stub` | **65/65** |
| `node v4/frame/theme/build-theme.mjs --check` | aktuell |
| `node v4/tools/geometry/geometry-v4.mjs --app knowledge,documents,notes,placements --widths 1280,420 --out v4/frame/lab/shots/geo-after` (ohne `--shared`: Theme kommt jetzt aus dem Rahmen) | knowledge/notes/placements gestylt montiert; Restfehler G4 (Ellipsen) bzw. notes 420 G1 (zwei innere Container 3 px/186 px zu breit) sind App-Befunde; documents siehe Befund oben |

Neue Tests (beide Modi, frische Seite): FR2-1 Container + Haltepunkte (1098/898/598 px → wide/mid/narrow) und doppelte
Deklaration · FR2-2 Zonen-Variablen · Zusatz data-app nur am root, Theme-Links und Tokens · FR2-3 Zug +80 px und
Pfeiltaste → 324 px in window_state, nach Shell-Tausch 324 px und neu verdrahtet, neues Fenster startet mit 348 px,
Schließen löscht den Schlüssel · FR2-7 RouteEvent bei offenem und geschlossenem Ziel, `ctx.route` + genau ein
`onRoute` auch bei spätem Abo, fehlendes Ziel → false · Zone-Aktion (Erfolg schließt, Fehler bleibt, Knopf in der Liste) ·
FR2-4 zwei Fenster: eigenes root/api, getrennte Eingaben, nach Shell-Tausch getrennt, Schließen von A lässt B bedienbar;
mock: `multi_instance:false` verhindert zweites Fenster · FR2-6 Ziehen + Alt+Pfeil, `shell_preferences.launcher_order` ·
FR2-5 Anheften/Lösen per Ziehen. Die Listener-/Observer-Zählung über 5 Shell-Tausche bleibt konstant.

Screenshots angesehen: `lab/shots/geo-after/{knowledge,notes,placements}/{1280,420}.png` (links keine Leerzone mehr,
gestylt ohne `--shared`, 420 schaltet auf die schmale Darstellung), Vorher: `lab/shots/geo-before-shared/`.
`lab/shots/integrated-stub-zone-action-1280.png` (Knopf „Auf aktuelle Version setzen“ rechts in der Zonenzeile),
`integrated-stub-fr2-launcher-1280.png`.

## Nachtrag Strang FR6a: Fensteranatomie im v4-Rahmen, 15.10.2026 (20:12–21:20)

Verlauf der Vorgaben (wichtig für die Nachfolger): Der Auftrag begann mit „eigene Titelleiste“. Der Owner hat das
während des Strangs zweimal korrigiert. (1) 20:2x „schwebende Icons sind gewollt, darunter keine Schaltelemente“:
keine Titelleiste, reservierte Zonen. (2) 20:3x „Icon-Header, Rahmen mit Eckerweiterungen, die zwei Hover-Icons
bleiben alle erhalten“: Das v4-Chrome ist jetzt exakt Shell-V2. Die Zwischenstände (kleines 36-px-Icon, heller Cluster
[App|Code][▢][×], Statusleiste) sind vollständig **zurückgebaut**. Es gab nie einen veröffentlichten Stand davon.

### Umgesetzt (Stand Abgabe)

| Punkt | Umsetzung | Dateien |
|---|---|---|
| Chrome = Shell-V2 | Wörtliche Kopie der Chrome-Regeln aus `app.css`: Basis `.shell-window`, Steuerknöpfe, Größe-Griffe und der V2-Block mit 80-px-Icon-Header, Kantenverlauf und Eckgriffen, Layout- und Schließen-Knopf. Die Inhalts-Akzent-Tokens (`--accent` usw.) sind bewusst nicht übernommen, der Inhalt gehört dem v4-Theme (sonst Kunden dunkel, TEXT-B). | `shared/shell-v2-chrome.css` (neu), `v4/frame/frame.css`, `index.js` (CSS-Liste: theme, base, shell-v2-chrome, frame) |
| G0 Shell-Treue | Das Rahmen-Labor rendert dasselbe Fenster-DOM einmal im v4-Rahmen und einmal nur mit `app.css` + `base.css` (app.js-Pfad) und vergleicht Icon-Header, vier Eckgriffe und Hover-Icons pixelweise (≥ 98 %). Mutation: ein auf 40 px verkleinerter Icon-Header ergibt 6 % Übereinstimmung. | `lab/frame.test.mjs` |
| Reservierte Zonen | Am Fenster: `--shell-icon-inset` 82 px × `--shell-icon-zone-height` 74 px (links) sowie `--shell-controls-inset` (Clusterbreite + 8, gemessen von `index.js`) × `--shell-controls-zone-height` 44 px (rechts). `--shell-v2-control-inset` zeigt darauf. Kit: `pane-heads.js` markiert direkte Kinder von `.ctox-pane-header` sowie `.ctox-filterbar`, die ein Zonenrechteck **schneiden**. Zeilen erhalten Innenabstand, einzelne Bedienelemente Außenabstand. Was unterhalb der Zonenhöhe liegt, bleibt in voller Breite (TEXT-B). Shell-Ansichten (Neue App, Code) beginnen unter 74 px. | `pane-heads.js`, `frame.css` |
| G8 | Geometrie-Labor: Kein interaktives App-Element schneidet die Zonen. Unit-Fixtures pos/neg und Mutant 5 sind grün. Rahmen-Labor: Lab-App `studio` (Nachbau Black Hole Studio, Leiste oben rechts) weicht aus, auch bei 420. Gegenprobe ohne Kit: Treffer gefunden. | `tools/geometry/measure.mjs`, `geometry-v4.mjs`, `geometry-v4.test.mjs`, `lab/apps/studio/1.0.0/index.js` |
| Icons aus einer Quelle (M-09/M-17) | `resolveIcon(app)` liefert je App-Version eine URL: `icon_svg` (xmlns ergänzt), `icon_asset` (bei Bedarf repariert) oder ein generiertes Icon (Verlauf aus app_id-Hash + Buchstabe, quadratisch). Launcher, Taskleiste und Fenster-Icon nutzen dieselbe URL. Das Fenster bekommt sie als Promise, das Bild bleibt bis zum Laden unsichtbar, Platzhalter in der App-Farbe. Kein Monogramm mehr. | `index.js`, `shared/window-manager.js` (iconAsset als Promise) |
| Palette nur berechnet (M-17, G9) | Die Palette kommt immer aus den gerenderten Icon-Pixeln (`decode()`, 48×48 auch ohne naturalWidth). Die Randfarben oben/links sind der Median eines schmalen Streifens am Logorand, durchsichtige Pixel werden über dem Icon-Hintergrund verrechnet. Cache je Icon-URL. `layout.frame_palette` wird nicht ausgewertet. `data-shell-v2-palette-source` ist `icon` oder `generated`. G9 per Pixelmessung im Rahmen-Labor: ΔE Logo-Rand zu Kante < 12 für notes, crm (icon_svg) und studio (generiert). Ein falsches `frame_palette` (Grün) ändert nichts. Mutation mit handgesetzter Kante: ΔE ≥ 12. | `shared/window-manager.js` (`shellV2EdgeJointsFromRgba`, `FRAME_PALETTE_CACHE`), `index.js` |
| Andock-Raster (M-01) | 3×3 räumlich (Ecken = Viertel, Kanten = Hälften, Mitte = Maximieren), darunter „Freie Größe“ und „Minimieren“. Deutsche Bezeichnungen, Hover öffnet nach 250 ms, Klick hält offen. Pfeiltasten räumlich, Enter wendet an, Esc schließt, aktuelle Lage hat `aria-current`. Kürzel Strg+Wahl+Pfeil (Hälften), Strg+Wahl+Enter (Maximieren), Strg+Wahl+U/I/J/K (Viertel). | `shared/window-manager.js` (`LAYOUT_GRID_ORDER`), `app.css` (Raster-Teil), `index.js`, `scripts/assert-shell-chat-composition.mjs`, `scripts/shell-v2-geometry-lab.mjs`, `shared/window-manager.test.mjs` |
| App ↔ Code (M-02) | Kein eigenes Quelltext-Fenster mehr. Ein Klick auf den Icon-Header öffnet das App-Menü (dieselbe Liste wie die Meldungen) mit „Code ansehen“ bzw. „Zurück zur App“, nur mit `apps.source.view`. Gewechselt wird im selben Fenster: Der App-Inhalt bleibt montiert, die Code-Ansicht bleibt beim Zurückschalten offen, Entwurf und Cursor bleiben (Test mit gleichem textarea-Element und gleicher Cursorposition). Esc führt zurück, Strg/Cmd+Umschalt+E schaltet um. `api.shell.openSource(appId)` holt das App-Fenster und schaltet auf Code. Alte Snapshots `shell:source:<app>` werden zum App-Fenster im Code-Modus. | `index.js`, `shell/status-zone.js` (`APP_MENU`) |
| Meldungen am Icon (M-14) | Das Badge am Icon-Header zeigt die Zahl, die Farbe folgt der höchsten Schwere (info blau, warn gelb, error rot). Ein Klick auf Icon oder Badge öffnet die Liste unter dem Icon im Fenster. Je Meldung stehen Text, die Aktion der App und „Ausblenden“ dort. Esc schließt. Eine neue Meldung löst einen kurzen Puls aus, die Liste öffnet sich dabei nicht selbst. Die FR4-Regeln gelten weiter. Eine Statusleiste unten gibt es nicht. | `shell/status-zone.js`, `frame.css` |
| Vollbild (M-11) | `api.shell.window.fullscreen()`, `exitFullscreen()` und `isFullscreen` setzen Element-Vollbild auf den Fensterinhalt. F11 und Strg+Cmd+F wirken auf das fokussierte Fenster, Esc beendet (Browser). `app-api.d.ts` ist ergänzt. In `modules/*` gibt es kein `requestFullscreen`, also nichts umzustellen. | `index.js`, `tools/port/app-api.d.ts` |
| Doppelklick | Ein Doppelklick auf eine freie Stelle eines Pane-Kopfs maximiert oder stellt wieder her. | `index.js` |
| Upload ohne Icon | Fehlt das Icon (`icon_svg` oder `icon_asset` samt Datei) nach dem Veröffentlichen, erscheint die Warnung `source.icon` in der Zone. | `source/upload.js` (`hasIcon`) |

Gefundener Fehler, mitbehoben: Die Code-Ansicht blieb nach einem Shell-Tausch leer. Ursache: Der `<link>` von
`source-mode.css` lud noch, als das Umschalten des Doppelpuffers fremdes Shell-CSS entfernte. Damit feuerte `load` nie.
Behoben, indem der Link sofort in `cssLinks` eingetragen wird. Davor schließt `stopShell` außerdem laufende Entwurfs-Schreibvorgänge ab (`flush`).

### Abnahme (selbst ausgeführt)
- `node src/apps/business-os/v4/frame/lab/frame.test.mjs mock`: 118/118 (Endlauf 21:07, inklusive FR5/FR6b-Prüfungen der Parallelstränge)
- `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 … frame.test.mjs stub`: 138/138 (Endlauf 21:07)
- `node --test shared/window-manager.test.mjs`: 15/15
- `node --test v4/tools/geometry/geometry-v4.test.mjs`: 17/17 (neu: G8 positiv, G8 negativ, Mutant 5)
- `v4/source/lab/accept.mjs --editor textarea`: ALLE BESTANDEN
- Geometrie-Lauf customers, support, notes bei 1280 und 420: G8 und G9 grün. notes war vor der Kit-Erweiterung für `.ctox-filterbar` rot (Suche unter dem Icon-Header) und ist jetzt grün. G4 ist bei 420 rot: customers kürzt den Titel „Kunden“, weil die einzeilige Kopfzeile links und rechts eingerückt wird. support war schon vorher rot.

### Offen bzw. nicht in diesem Strang erledigt
- Zustandsadapter §16.45 (`kernel.metrics.collections` → Badge-Farbe, „Neu verbinden“ → `kernel.reconnect()`, V3-Rückfall `ctoxBusinessOsSyncDiagnostics`): nicht gebaut.
- gen-manifest: `frame_palette` nicht mehr übernehmen, Wächter-Befund, Raster-Icons aus `shared/assets/workjet-icons/*` als `icon_asset` für alle 41 Apps: nicht gebaut. Der Rahmen ignoriert `frame_palette` bereits.
- „Problem melden“ und „Wunsch äußern“ im App-Menü: `v4/shell/report.js` (FR6b) lag zum Zeitpunkt nicht vor. Einhängen über `APP_MENU` in `index.js` `attachChrome`.
- Neuer Stub-Pin 74137b1: Der stub-Lauf wirft dort eine Ausnahme. Zusätzlich steigt der Observer-Zähler je Shell-Tausch um 1. Noch nicht analysiert, Abnahme-Pin bleibt 68b83a3.
- Zwischendurch (21:0x) war FR3-2 rot, weil die Taskleiste von FR6b `data-v4-group` mitgezählt hat. Im Endlauf ist es grün (von FR6b behoben).
- `scripts/assert-shell-chat-composition.mjs` ist auf das Raster umgestellt, aber nicht ausgeführt (braucht eine laufende Shell). `scripts/shell-v2-geometry-lab.mjs` lief im Klon nicht (`ERR_MODULE_NOT_FOUND` playwright). Beide nur per `node --check` geprüft.
- G4 bei 420 (customers, Titel „Kunden“ gekürzt): Folge der Zonen auf beiden Seiten einer einzeiligen Kopfzeile. Die App müsste ihre Aktionen bei schmaler Breite in eine zweite Zeile umbrechen.
- Bei den Lab-Icons mit runden Ecken (notes/tasks/ledger, `rx=14`) beginnt die Kante in der Farbe des Icon-Hintergrunds, also dunkel. Das ist nahtlos, aber unschön. Die echten Raster-Icons sind quadratisch.

## FR6b (15.10.2026): Taskleiste je App, Desktop anzeigen, Meldeschleife (Shell-Seite)

Protokoll und Belege: `v4/frame/lab/FR6b.md`.

### `reportProblem` – Melden aus jedem Fenster (für FR6a, App-Menü am Icon-Header)

```js
import { reportProblem } from '../shell/report.js';   // aus v4/frame/index.js auch als Re-Export: frame.reportProblem
await reportProblem({ appId: rec.appId, kind: 'bug' | 'wish', windowId: rec.ownerId /* optional */ });
// → { report_id, report, bug } | null   (null: Abbruch, kein Fenster der App, oder Speichern gescheitert)
```

- Ziel ist das angegebene Fenster (`windowId` = `ownerId` oder Fenster-ID), sonst das zuoberst liegende Fenster der App.
  Das Fenster kommt nach vorn; die Abfrage läuft als Blatt IM Fenster über dasselbe Fensterblatt wie `api.shell.dialog`
  (`rec.host.win.sheet`, `window-sheet.js`): Schritt 1 „Was ist passiert?“ (`wish`: „Was wünschst du dir?“),
  Schritt 2 „Was hast du erwartet?“, beide mehrzeilig, Pflicht. Esc/Abbrechen speichert nichts.
- Angehängt (`client_context`): `app_id`, `app_title`, `window_title`, `app_ver`, `window_id`, `record` (markierter
  Datensatz im Fenster: `[data-context-record-id|data-record-id]` mit `aria-selected`/`aria-current`/`.is-selected`/
  `.is-active`, sonst `null`), `recent_errors` (aktuelle Fehler/Warnungen der Zustandszone, höchstens 5),
  `window_size`, `window_image: null` (kein Fensterbild ohne Fremdbibliothek, siehe FR6b.md).
- Gespeichert wie bisher „Bugs & Features“ (`modules/reports` liest beide): je eine Zeile in `business_module_reports`
  und `ctox_bug_reports` (Felder wie `shared/business-reporter.js` `persistLocalBusinessReport`), über `api.put` der
  Shell-App-API. `kind`: `bug` → `bug`, `wish` → `feature`. Fehler → ein Zoneneintrag `shell.report` (error) im Fenster.
- Tastenkürzel: Strg+Alt+B (Mac: Ctrl+Opt+B) im vorderen App-Fenster (nicht in Shell-Ansichten wie „Neue App“).
- App-Menü am Icon-Header (FR6a, `APP_MENU` in `attachChrome`): „Problem melden“ (`report-bug`, Hinweis Strg+Alt+B) und
  „Wunsch äußern“ (`report-wish`) für ALLE Nutzer; „Code ansehen“/„Zurück zur App“ bleiben an `apps.source.view`.
- **Offen (Manifest, nicht im FR6b-Schreibbereich):** `v4/manifests/shell.manifest.json` deklariert nur
  `shell_preferences`. Für den echten Kern braucht die Shell `foreign_collections.business_module_reports` und
  `ctox_bug_reports` mit `ops: ["write"]` (Eigentümer `ctox`). Der Stub prüft das nicht.

### Taskleiste: Gruppen

- Gruppe = `layout.presentation.taskbar_group` (Zeichenkette) der App, sonst die App selbst. Überschrift der Gruppe =
  `layout.presentation.taskbar_group_title`, sonst der Schlüssel mit großem Anfangsbuchstaben (`crew` → „Crew“).
  `presentation` übersteht `normalizeLayout` (kernel-adapter `LAYOUT_KEYS`). Crew-Chat-Apps setzen
  `presentation: { taskbar_group: 'crew' }`; kein App-Sonderfall im Rahmen.
- Runde 2: Pin und Fenster derselben App sind EIN Eintrag (Windows). Angeheftet ohne Fenster = nur Icon; mit Fenstern
  Titel + Laufend-Markierung (`.v4f-task--win`, `.is-open`), ab 2 Fenstern die Zahl. Reihenfolge: angeheftete in
  Pin-Reihenfolge, dann laufende. Angeheftete Mitglieder einer Gruppe (z. B. `crew-anna`) verschmelzen mit „Crew“.
  Der getrennte Pin-Block `[data-v4-pins]` entfällt.
- DOM: alle Einträge in `[data-v4-wins]`: `[data-v4-task-group=<schlüssel>]` mit `data-v4-pin=<appId>` (nur angeheftet,
  ziehbar), `data-v4-win-ids` (Leerzeichen-getrennt), `data-v4-app-ids`, `data-v4-count`; `data-v4-win` nur bei genau
  einem Fenster. Fensterliste: `[data-v4-task-menu]` mit `[data-v4-task-menu-win=<fenster-id>]`. Icon-Modus:
  `.v4f-bar.is-compact`. Überlauf: Knopf `[data-v4-task-more]` „»“ am Ende, versteckte Einträge `.is-overflow`,
  Liste mit `[data-v4-task-menu-entry=<schlüssel>]`. Die Leiste schneidet nie ab.
- `taskGroupOf(app)` (taskbar.js) ist die eine Gruppenregel für Pins und Fenster (auch `windowList` in index.js).
- `createTaskbar({..., onFocusWindow})` neu (fokussieren ohne Minimieren); `taskbar.disconnect()` löst den
  ResizeObserver beim `stopShell` (Element bleibt für den Doppelpuffer stehen).

### Desktop anzeigen

- `showDesktop()` (Knopf „Desktop“ links in der Taskleiste, Strg+Alt+D): sichtbare Fenster minimieren und merken
  (`state.desktopRestore`, Stapelreihenfolge); nächster Aufruf stellt genau diese wieder her (vorher schon minimierte
  bleiben unten). Nichts sichtbar und nichts gemerkt → Suche im Launcher fokussieren.
