# Shell v4 · App-Host mit Live-Remount und Zustandszone – Prototyp und Messung

> **Nachtrag Strang I (15.10.):** `shell/shell.js` und `window-chrome.css` sind entfallen. Die Messung läuft jetzt gegen die integrierte Shell (`v4/frame/index.js` + `shell/app-host.js`, mount-Frist 9 s, Doppelpuffer). Der automatische Messwert-Block unten stammt aus diesem Stand, der Fließtext darüber aus P2. Siehe `v4/frame/INTEGRATION.md`.


Strang P2 (Entwurf §5–§7, Architektur §9.3, §10.2/§10.2a, §16.7, §16.10). Stand 15.10.2026, ca. 13:05.
Klon `~/Documents/ctox-shell-v4`, Zweig `shell-v4/prototype`, kein Commit (Auftrag: kein git).

**Umgebung aller Zahlen:** MacBook (darwin arm64, 10 CPUs), chromium-headless-shell 148.0.7778.96 (Playwright 1.60),
zwei Tabs im selben Browser-Kontext, Laborserver auf 127.0.0.1. Load-Average während der Läufe **54–69** (fremde Last
anderer Sitzungen); die Spannweiten unten stammen aus drei vollständigen Läufen (12:47, 12:49, 12:50) plus Endlauf (13:00).
Alle Zeiten sind lokale Zeiten ohne Netz und ohne Service Worker: App-Code kommt per `import()` vom Laborserver
(versionierte Pfade `/~a/<app>@<ver>/…`, gecacht), der Artefakt-Transfer über Spur 4 ist nicht enthalten.

## Kurzfassung

| Szenario | Ergebnis (Endlauf) | Spannweite über 4 Läufe | Bewertung |
|---|---|---|---|
| (a) Tab B tippt, Tab A veröffentlicht v2/v1 im Wechsel, 20 Wdh. | Veröffentlichen → mounted: **B p50 4,6 ms / p95 12,3 ms**, A (Leiter) p50 2,3 / p95 3,2 ms; 0 Abweichungen bei Wert, Fokus, Auswahl, Scroll (Textarea + Liste); jedes Mal neuer `root` | B p50 4,3–4,7 ms, p95 5,6–12,3 ms; 1. Remount (kalt, Modul+CSS erstmals geladen) 16,6–304 ms | bestanden |
| (a) Phasen je Remount in B (p50) | Snapshot 0,1 ms · window_state 0,4 ms · Laden 0,2 ms · unmount 0,9 ms · mount 0,9 ms | Laden kalt bis 227 ms | – |
| (a-Stub) dasselbe gegen `createStubKernel` (latencyMs 20) über `shell/kernel-adapter.js`, Blob-Ladeweg | **B p50 8,7 ms / p95 14,6 ms**, A p50 7,6 / p95 12,8 ms; 0 Abweichungen | 1 Lauf | bestanden |
| (b) App ohne `snapshot()`: generische Sicherung | input, select, checkbox, textarea + Fokus/Auswahl + 2 Scrollpositionen erhalten; 87,7 ms | 6,6–87,7 ms | bestanden |
| (c) v3: `mount()` hängt | Rückfall auf v2 nach **10 217 ms** (B), Zone: „Version 3 startet nicht (mount() nicht binnen 10 s fertig); 2 läuft weiter“; Eingabe erhalten; erneutes Veröffentlichen von v2 leert die Zone | 10 015–10 217 ms | Rückfall korrekt, **≤ 10 s nicht erreicht** (siehe Lücken) |
| (c-Stub) dasselbe gegen den Kern-Stub | Rückfall nach 10 017 ms, Eingaben erhalten; **Aktivierungszeiger des Stubs bleibt auf der kaputten Version** | 1 Lauf | Schnittstellenlücke |
| (c2) v4: `mount()` wirft | Rückfall nach 15,3 ms, Fehler in der Zone, Eingabe erhalten | 15,3–17,9 ms | bestanden |
| (d) Live-Datenänderung Tab A → Liste in Tab B | **p50 1,7 ms / p95 2,6 ms** (20 Wdh.); eigener Tab synchron (p50 0,2 ms); Befehl in B beobachtet: queued→accepted→running→done | p50 1,3–1,7, p95 1,8–2,6 ms | bestanden |
| (e) Shell-Live-Tausch 1→2, 3 Fenster, halbes Formular | stop 1,4 + import 30,9 + start 34,3 = **66,6 ms**; Geometrie, Werte, Fokus, Auswahl, Scroll aller 3 Fenster gleich; neue Shell remountet danach weiter (6,7 ms) | 12,6–66,6 ms | bestanden (Fläche während des Tauschs leer, s. u.) |
| (f) Zustandszone | leer: `hidden`, 0 px; 3 Einträge: genau 1 Zeile (23 px inkl. 1 px Rand), schwerster = error, gekürzt mit „…“; Liste öffnet im Fenster (Rechteck innerhalb, Elternelement = Fenster); Aktivierung 1 s: nie sichtbar, 3 s: bei 1,7 s unsichtbar, bei 2,4 s sichtbar, danach leer; Verbindung weg: bei 4,5 s unsichtbar, bei 5,8 s „Keine Verbindung zum Server“ in A und allen 3 Fenstern von B; Outbox > 10 s gemeldet; nach Wiederverbindung leer | stabil | bestanden |
| (h) 10 Fenster in Tab B, 9× formular gleichzeitig | Veröffentlichen → alle 9 mounted **p50 12,4 ms / p95 19,3 ms** (6 Wdh.); Werte erhalten; neues CSS geladen, bevor altes entfernt wurde | p50 11,4–12,9 ms | bestanden |
| (g) kein Reload, kein Datenabruf | 38 Anfragen: 2 document, 23 script, 13 stylesheet – nur `/`, `/lab/*.js`, `/~a/*@*/…`; 0 fetch/xhr/websocket; je Tab genau 1 Hauptframe-Navigation, Seiten-Sentinel unverändert; 0 Konsolenfehler | stabil | bestanden |

Bildbelege: `shots/a-tab-b-nach-20-remounts.png`, `shots/c-rueckfall-zone-tab-b.png`, `shots/e-nach-shell-tausch-tab-b.png`,
`shots/f-zone-liste-offen-tab-a.png`, `shots/h-zehn-fenster-tab-b.png` (selbst angesehen: c zeigt die rote Zeile unten
im Fenster mit erhaltener Eingabe, f die offene Liste innerhalb des Fensters über der Zone).

## Was gebaut ist

- `v4/shell/app-host.js` – Fenster-Chrome (Icon, Titel, Kopfzeilen-Slot, Schließen, Zone), App-Root
  (`contain: layout paint; overflow: clip`), `api.shell` (setTitle, registerHeader(el), navigate/onRoute, assetUrl,
  locale, t, window.close), `api.status` → Zone, Zustandsfolge `mounting → mounted → swapping → mounted | rolled-back`
  (+ `failed`, s. u.), Swap nach Vertrag: snapshot (≤ 1 MiB, structured clone geprüft) bzw. generische Sicherung →
  `window_state` → neues CSS laden, erst dann altes weg → unmount → mount mit `{restored}` → 10-s-Frist → Rückfall mit
  demselben Snapshot. Zusätzlich: neue Version wird vor dem Abbau importiert (scheitert der Import, bleibt die alte
  Instanz unberührt); verspätet fertiges `mount` wird sofort abgebaut; die API einer abgelösten Instanz ist wirkungslos
  und alle ihre Abonnements werden vom Host beendet (auch vergessene); Passwort- und Datei-Felder werden nie gesichert.
- `v4/shell/status-zone.js` – Zone (unsichtbar ohne Einträge, eine Zeile, Liste im Fenster), Aktivierungsfilter
  (2-s-Regel, failed sofort), Messwert-Wächter (Verbindung > 5 s, Outbox > 10 s, Rückstand wächst > 10 s).
- `v4/shell/shell.js` – `startShell(kernel, {restored})` / `stopShell() → snapshot`, Fensterfläche, Ziehen,
  Shell-weite Einträge in jede Zone, Abonnement auf `activation.onChange`.
- `v4/shell/window-chrome.css` – Chrome und Zone (hell/dunkel).
- `v4/shell/kernel-adapter.js` – Brücke vom verbindlichen Kern-Stub auf die Host-Schnittstelle, inkl. Blob-Ladeweg aus
  `artifacts.read(hash)`.
- `v4/lab/app-host/` – `api-attrappe.js` (eigene Kernel-Attrappe: IDB docs/meta/outbox/window_state, Leiter per
  `navigator.locks` mit `leader_gen`, BroadcastChannel + `local_seq`-Abgleich alle 2 s, Mango-Teilmenge mit mehrfeldrigem
  Sort, WriteHandle, simulierte Befehlsfolge, Messwerte), Demo-Apps `formular` v1–v4 und `notizen` v1/v2,
  `server.mjs`, `loader.js`, `index.html`, `measure.mjs`, `measure-stub.mjs`, `results.json`, `results-stub.json`, `shots/`.

## Was nicht funktioniert / Lücken

1. **Rückfall „≤ 10 s“ wird nicht erreicht**: Die 10-s-Frist läuft ab dem `mount()`-Aufruf; danach kommen Wiedermontage
   der alten Version und (unter Last) Zeitgeber-Verzug: gemessen 10 015–10 217 ms ab Veröffentlichen. Vertrag
   („mount nicht binnen 10 s“) und Messziel („Rückfall binnen ≤ 10 s“) widersprechen sich. Vorschlag: Frist ab
   Swap-Beginn mit Reserve (z. B. 9,5 s für mount), oder Ziel als „mount-Frist 10 s, Rückfall ≤ 10,5 s“ festschreiben.
2. **Aktivierungszeiger nach Rückfall**: Die Shell fällt zurück, der Kernel bleibt aber auf der kaputten Version
   (Attrappe und Stub, gemessen: `{"ver":"99","state":"activated"}`). Ein danach geöffnetes Fenster würde wieder die
   kaputte Version starten; ein Fenster ohne Vorgänger endet im Zustand `failed` (nicht im Vertrag) mit Fehler in der
   Zone. Der Stub setzt den Zeiger nur bei seinem eigenen, kernelseitigen Remount zurück (`revertActivation`).
3. **Shell-Tausch zeigt kurz eine leere Fläche** (12–67 ms): `stopShell()` baut alle Fenster ab, `startShell()` baut sie
   neu. App-CSS wird dabei entfernt und neu geladen. Für die Produktion: Chrome und Inhalt der alten Shell stehen
   lassen, neue Shell unsichtbar montieren, dann in einem Schritt tauschen.
4. **Kurzes Doppel-CSS beim App-Swap**: Zwischen „neues CSS geladen“ und „altes entfernt“ liegt der Modul-Import
   (await); beide Stylesheets gelten kurz für denselben Selektor `[data-app="formular"]`. Visuell nicht geprüft
   (Messung nur über die Reihenfolge). Vorschlag: App-CSS je Version scopen (`[data-app][data-app-ver]` bzw. `@scope`,
   vom SW beim Ausliefern eingesetzt).
5. Generische Sicherung kennt nur Elemente mit `id`/`name` (Radio-Gruppen über `name`+`value`); `contenteditable`,
   Elemente in virtuellen Listen und doppelte ids gehen verloren. Ein hängendes `mount` kann nicht abgebrochen werden:
   sein Code läuft weiter (Timer, globale Listener); der Host entzieht nur API und DOM.
6. Nicht gemessen: echter Hintergrund-Tab (headless sind alle Tabs sichtbar), Safari/Electron/Mobil, Service-Worker-Weg
   (Strang P1), große echte Apps, `navigate`/`onRoute` und `registerHeader` nur funktional, nicht im Messskript.
   `measure-stub.mjs` deckt nur (a) und (c) ab.

## Offene Punkte für die Produktion

- **Hintergrund-Tabs**: Chrome drosselt Zeitgeber verborgener Tabs (bis 1/min bei intensiver Drosselung) und hält
  `requestAnimationFrame` an. Eine App, die in `mount` auf rAF wartet, würde dort in den 10-s-Rückfall laufen.
  Empfehlung: Remount verborgener Tabs bis `visibilitychange` aufschieben (Snapshot sofort sichern) oder die Frist nur
  im sichtbaren Zustand zählen.
- `window_state` enthält ungesicherte Nutzereingaben (personenbezogen). Aufbewahrung festlegen: löschen nach
  bestätigtem Mount + Karenz, beim Abmelden mit der Identität (§16.7) räumen.
- Fokus: wird nur in dem Dokument zurückgegeben, das ihn hatte; bei mehreren Fenstern im selben Tab gewinnt das Fenster,
  dessen Snapshot den Fokus trägt (im Test nur eines).
- Kalter erster Remount (Modul + CSS erstmals laden) 16–304 ms gegenüber 4 ms warm: Vorladen der neuen Version schon bei
  `fetched`/`data-ready` (Modul-Import + CSS als `preload`) senkt die sichtbare Zeit; dann ist `activated` nur noch
  Umschalten.
- Remount aller Fenster einer App läuft parallel (9 Fenster p50 12,4 ms); der Stub remountet kernelseitig sequenziell.
- CI-Wächter: Zone-Vokabular, kein `location.reload`, kein `fetch` in Apps – (g) ist die Laufzeit-Gegenprobe.

## Schnittstellenwünsche an den Kernel (und Abweichungen zum Stub)

Abweichungen meiner Attrappe/Host-Schnittstelle zum verbindlichen Stub (`~/.local/state/ctox-sync-v4/stub/index.d.ts`),
überbrückt in `shell/kernel-adapter.js`:

| Thema | Stub | Shell-Host braucht / Wunsch |
|---|---|---|
| Wer montiert | `shell.mount(root, appId)` + kernelseitiger Remount (`scheduleRemount`, sequenziell, gleicher root, ohne generische Sicherung, ohne CSS) und kernelseitige Zone `statusZone` je App | App-Host, Remount und Zone sind laut Entwurf §3/§5/§7 **Shell**-Eigentum. Wunsch: `shell.mount` als „Stub-Komfort“ kennzeichnen und nicht in den echten Kernel übernehmen; Kernel liefert nur Ereignisse und Daten. |
| Rückfall melden | Zeiger-Rücknahme nur im eigenen Remount (`revertActivation`) | **`activation.reportFailed(appId, ver, reason)`** (= `app_activation{failed}`, §9.3/Anf. 69) für die Shell; Kernel setzt `activated_ver` zurück, alle Tabs folgen. Gemessen: ohne das bleibt der Zeiger auf der kaputten Version. |
| `window_state` | nur `sim.windowState` lesend | `windowState.put/get/delete(key)` in der Shell-Kernel-API (Store existiert in IDB v5); Adapter nutzt bis dahin eine eigene IDB. |
| `api.shell` / `api.status` | baut der Kernel in `appApi` (`registerHeader(spec)` speichert ein Datenobjekt, Zone je App) | Shell setzt beide; Wunsch: `appApi(appId, ver, { shell, status })` mit Einschub durch die Shell, oder Datenteil ohne `shell`/`status`. `registerHeader(el: HTMLElement)` (Brief/Entwurf) vs. `spec` (Stub) muss entschieden werden. |
| Zone-Einträge des Kernels | `statusZone.forApp(appId)` (je App, je Tab) | passt; Host spiegelt `source:'kernel'`-Einträge in jedes Fenster der App (`kernel:<code>`). Wunsch: `ref` auf Dokument/Write, damit die Shell zum Fenster mit dem Datensatz zuordnen kann. |
| Aktivierungsereignis | `{app_id, ver, state, activated_ver, error}` | passt (Adapter mappt). Wunsch: Zeitstempel `t0`/`published_at` der Veröffentlichung im Ereignis, damit „Veröffentlichen → mounted“ im Feld messbar wird. |
| Aktuelle Version | `activation.get(appId)` LiveDoc | passt; Shell nutzt `activated_ver`. |
| Manifest | async, ohne `styles`, ohne `title`/`icon` | Adapter leitet CSS aus `files` (*.css) ab, Titel aus `apps`. Wunsch: `styles[]` (Reihenfolge!) und `icon` im Manifest. |
| Code laden | `artifacts.read(hash) → Blob`; keine URLs | passt für den Blob-Rückfall (gemessen, a-Stub). Regelweg `/~a/<app>@<ver>/` = Service Worker (P1). |
| `metrics.link.state` | Stub `online/offline`, Attrappe `up/down` | Wertemenge im Vertrag festlegen; Zone behandelt `up/online/connected` als verbunden. |
| Leiter | `{isLeader, leader_gen, tab_id}` | passt. |

## Messwerte

<!-- MESSWERTE:START -->
_Automatisch von measure.mjs, Lauf 2026-10-15T19:36:40.184Z, chromium-headless-shell 148.0.7778.96, darwin arm64, 10 CPUs, Node v26.6.0._

### a-remount-zwei-tabs — BESTANDEN

20 Remounts v1↔v2 in beiden Tabs; Veröffentlichen→mounted B p50 7.4 ms / p95 10.6 ms, A p50 6.4 / p95 8.8 ms; Abweichungen Wert/Fokus/Auswahl/Scroll: 0.

```json
{
 "publishToMountedMs": {
  "tabA_leader": {
   "n": 20,
   "p50": 6.4,
   "p95": 8.8,
   "min": 4.4,
   "max": 17.9
  },
  "tabB_reader": {
   "n": 20,
   "p50": 7.4,
   "p95": 10.6,
   "min": 5.2,
   "max": 18.5
  }
 },
 "rawB": [
  18.5,
  9,
  9.6,
  8.4,
  10.6,
  8.5,
  6.6,
  8.2,
  9.5,
  7.7,
  6.6,
  7.1,
  7.7,
  7,
  7.4,
  6.2,
  5.4,
  7.2,
  5.2,
  6
 ],
 "phasesB_first2": [
  {
   "toSwapStart": null,
   "snapshot": 0.7,
   "persist": 1.1,
   "load": 6.8,
   "unmount": 1.7,
   "mount": 2.9
  },
  {
   "toSwapStart": null,
   "snapshot": 0.7,
   "persist": 1.2,
   "load": 0.3,
   "unmount": 2.2,
   "mount": 1.1
  }
 ],
 "phaseStatsB": {
  "snapshot": {
   "n": 20,
   "p50": 0.3,
   "p95": 0.7,
   "min": 0.1,
   "max": 0.7
  },
  "persist": {
   "n": 20,
   "p50": 0.2,
   "p95": 1.2,
   "min": 0.1,
   "max": 2.1
  },
  "load": {
   "n": 20,
   "p50": 0.2,
   "p95": 0.3,
   "min": 0,
   "max": 6.8
  },
  "unmount": {
   "n": 20,
   "p50": 1.4,
   "p95": 2.1,
   "min": 1,
   "max": 2.2
  },
  "mount": {
   "n": 20,
   "p50": 1.4,
   "p95": 2.9,
   "min": 0.9,
   "max": 3.3
  }
 },
 "mismatches": [],
 "finalState": {
  "ver": "1",
  "focus": "beschreibung",
  "selection": [
   90,
   90
  ],
  "scroll": {
   "beschreibung": 120,
   "liste": 200
  },
  "nameValue": "Erika Mustermann",
  "beschreibungLen": 677,
  "prio": null
 }
}
```

### b-generische-sicherung — NICHT BESTANDEN

Abbruch: page.check: Timeout 30000ms exceeded.
Call log:
  - waiting for locator('[data-window-id="v4app:notizen#2"] input[name="wichtig"]')
    - locator resolved to <input name="wichtig" type="checkbox"/>
  - attempting click action
    2 × waiting for element to be visible, enabled and stable
      - element is visible, enabled and stable
      - scrolling into view if needed
      - done scrolling
      - <div tabindex="0" role="button" data-window-drag-region="" class="shell-window-v2-icon" aria-label="Notizen verschieben" aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight">…</div> intercepts pointer events
    - retrying click action
    - waiting 20ms
    2 × waiting for element to be visible, enabled and stable
      - element is visible, enabled and stable
      - scrolling into view if needed
      - done scrolling
      - <div tabindex="0" role="button" data-window-drag-region="" class="shell-window-v2-icon" aria-label="Notizen verschieben" aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight">…</div> intercepts pointer events
    - retrying click action
      - waiting 100ms
    58 × waiting for element to be visible, enabled and stable
       - element is visible, enabled and stable
       - scrolling into view if needed
       - done scrolling
       - <div tabindex="0" role="button" data-window-drag-region="" class="shell-window-v2-icon" aria-label="Notizen verschieben" aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight">…</div> intercepts pointer events
     - retrying click action
       - waiting 500ms


```json
{
 "stack": [
  "page.check: Timeout 30000ms exceeded.",
  "Call log:",
  "  - waiting for locator('[data-window-id=\"v4app:notizen#2\"] input[name=\"wichtig\"]')",
  "    - locator resolved to <input name=\"wichtig\" type=\"checkbox\"/>",
  "  - attempting click action",
  "    2 × waiting for element to be visible, enabled and stable"
 ]
}
```

### c-rueckfall-v3 — BESTANDEN

v3 (mount hängt, Frist 9 s) → Rückfall auf v2 nach 9018.2 ms (B) / 9022.9 ms (A), ≤ 10 s: true; reportFailed gemeldet (2×), Zeiger danach 2 (zurückgenommen); Zone: „Version 3 startet nicht (mount() nicht binnen 9 s fertig); 2 läuft weiter“; Eingabe-Abweichungen: 0; nach erneutem Veröffentlichen von v2 Zone leer: true.

```json
{
 "publishToRolledBackMs": {
  "A": 9022.9,
  "B": 9018.2
 },
 "ver": "2",
 "zone": {
  "hidden": false,
  "level": "error",
  "text": "Version 3 startet nicht (mount() nicht binnen 9 s fertig); 2 läuft weiter",
  "inChrome": true
 },
 "inputDiffs": [],
 "pointerAfterRollback": {
  "ver": "2",
  "state": "activated",
  "reverted": true
 },
 "reportFailed": [
  {
   "ver": "3",
   "tab": "A"
  },
  {
   "ver": "3",
   "tab": "B"
  }
 ],
 "afterForwardRollback": {
  "hidden": true,
  "entries": []
 }
}
```

### c2-rueckfall-mount-wirft — BESTANDEN

v4 (mount wirft) → Rückfall auf v2 nach 12 ms; Zone: „Version 4 startet nicht (v4: Pflichtfeld-Schema fehlt); 2 läuft weiter“; Abweichungen 0.

```json
{
 "publishToRolledBackMs": 12,
 "zone": {
  "level": "error",
  "text": "Version 4 startet nicht (v4: Pflichtfeld-Schema fehlt); 2 läuft weiter"
 },
 "diffs": []
}
```

### d-live-daten — BESTANDEN

20 Schreibvorgänge Tab A → Liste in Tab B sichtbar p50 2.2 ms / p95 4.1 ms (BroadcastChannel nach IDB-Commit); Befehl in B gesehen: queued→accepted→running→done.

```json
{
 "putToOtherTabDomMs": {
  "n": 20,
  "p50": 2.2,
  "p95": 4.1,
  "min": 1.9,
  "max": 8.5
 },
 "putToOwnTabDomMs": {
  "n": 20,
  "p50": 0.2,
  "p95": 0.3,
  "min": 0.2,
  "max": 0.8
 },
 "commandStatesSeenInB": {
  "states": [
   "queued",
   "accepted",
   "running",
   "done"
  ],
  "ms": 442.60009765625
 }
}
```

### e-shell-live-tausch — BESTANDEN

Shell 1→2 (Doppelpuffer) mit 3 Fenstern (2× formular v2, 1× notizen v2), halbes Formular: stop 2.3 ms + import 20.2 ms + start/umschalten 47.1 ms = 69.6 ms; Abweichungen 0; nie leer: min. sichtbare Fenster mit Inhalt 3 über 29 Mutationen, 3 über 9 Frames; Remount nach Tausch 10.3 ms.

```json
{
 "swap": {
  "stopMs": 2.3,
  "importMs": 20.2,
  "startMs": 47.1,
  "totalMs": 69.6,
  "windows": 3,
  "snapshotBytes": 2496,
  "probe": {
   "moSamples": 29,
   "moMin": 3,
   "rafSamples": 9,
   "rafMin": 3
  }
 },
 "diffs": [],
 "shellState": {
  "ver": "2",
  "desktops": 1,
  "desktopVer": "2",
  "shellCss": [
   "http://127.0.0.1:60053/~a/shell@2/v4/frame/theme/theme.css",
   "http://127.0.0.1:60053/~a/shell@2/shared/base.css",
   "http://127.0.0.1:60053/~a/shell@2/shared/shell-v2-chrome.css",
   "http://127.0.0.1:60053/~a/shell@2/v4/frame/frame.css",
   "http://127.0.0.1:60053/~a/shell@2/shared/shell-v2-topbar.css"
  ],
  "appCss": [
   "/~a/notizen@1/index.css",
   "/~a/formular@2/index.css"
  ]
 },
 "remountAfterSwapMs": 10.3,
 "focusBefore": "name",
 "focusAfter": "name"
}
```

### f-zustandszone — NICHT BESTANDEN

Abbruch: page.click: Timeout 30000ms exceeded.
Call log:
  - waiting for locator('[data-window-id="v4app:formular#1"] .v4-status-zone__line')
    - locator resolved to <span class="v4-status-zone__top v4-status-zone__line">Speichern fehlgeschlagen: Feld „Name“ ist Pflicht…</span>
  - attempting click action
    2 × waiting for element to be visible, enabled and stable
      - element is visible, enabled and stable
      - scrolling into view if needed
      - done scrolling
      - <label>…</label> from <div data-window-content="" class="shell-window-content v4f-content">…</div> subtree intercepts pointer events
    - retrying click action
    - waiting 20ms
    2 × waiting for element to be visible, enabled and stable
      - element is visible, enabled and stable
      - scrolling into view if needed
      - done scrolling
      - <label>…</label> from <div data-window-content="" class="shell-window-content v4f-content">…</div> subtree intercepts pointer events
    - retrying click action
      - waiting 100ms
    58 × waiting for element to be visible, enabled and stable
       - element is visible, enabled and stable
       - scrolling into view if needed
       - done scrolling
       - <label>…</label> from <div data-window-content="" class="shell-window-content v4f-content">…</div> subtree intercepts pointer events
     - retrying click action
       - waiting 500ms


```json
{
 "stack": [
  "page.click: Timeout 30000ms exceeded.",
  "Call log:",
  "  - waiting for locator('[data-window-id=\"v4app:formular#1\"] .v4-status-zone__line')",
  "    - locator resolved to <span class=\"v4-status-zone__top v4-status-zone__line\">Speichern fehlgeschlagen: Feld „Name“ ist Pflicht…</span>",
  "  - attempting click action",
  "    2 × waiting for element to be visible, enabled and stable"
 ]
}
```

### h-zehn-fenster-und-css-reihenfolge — BESTANDEN

10 Fenster in Tab B (9× formular gleichzeitig): Veröffentlichen→alle mounted p50 42.8 ms / p95 55.2 ms (6 Wdh.); Werte erhalten: true; CSS neu geladen vor Entfernen alt: true; danach 2 App-CSS-Links.

```json
{
 "windowsInTabB": 10,
 "formularWindows": 9,
 "publishToAllMountedMs": {
  "n": 6,
  "p50": 42.8,
  "p95": 55.2,
  "min": 39.9,
  "max": 55.2
 },
 "raw": [
  44,
  42.8,
  44.3,
  39.9,
  55.2,
  40.5
 ],
 "valuesKept": true,
 "cssOrderNewLoadedBeforeOldRemoved": true,
 "cssLogFirst6": [
  [
   "add",
   "/~a/formular@2/index.css",
   73058
  ],
  [
   "load",
   "/~a/formular@2/index.css",
   73058
  ],
  [
   "remove",
   "/~a/formular@1/index.css",
   73093
  ],
  [
   "add",
   "/~a/formular@1/index.css",
   73130
  ],
  [
   "load",
   "/~a/formular@1/index.css",
   73130
  ],
  [
   "remove",
   "/~a/formular@2/index.css",
   73165
  ]
 ],
 "appCssLinksAfter": 2,
 "usedJsHeapMiB": 10
}
```

### g-kein-reload-kein-datenabruf — BESTANDEN

102 Anfragen gesamt (document 2, script 76, stylesheet 24), davon unerwartet/Daten: 0; Hauptframe-Navigationen A=1, B=1 (je 1 = nur der erste Aufruf); Seiten-Sentinel unverändert: true; Konsolenfehler: 0.

```json
{
 "resourceTimingTabB": [
  {
   "url": "/~a/shell@1/v4/frame/index.js",
   "startMs": 16,
   "durationMs": 0.4
  },
  {
   "url": "/~a/shell@1/shared/taskbar-pins.js",
   "startMs": 18,
   "durationMs": 2.5
  },
  {
   "url": "/~a/shell@1/shared/window-manager.js",
   "startMs": 18,
   "durationMs": 2.9
  },
  {
   "url": "/~a/shell@1/v4/shell/kernel-adapter.js",
   "startMs": 18,
   "durationMs": 4.1
  },
  {
   "url": "/~a/shell@1/v4/shell/app-host.js",
   "startMs": 18,
   "durationMs": 3.5
  },
  {
   "url": "/~a/shell@1/v4/shell/status-zone.js",
   "startMs": 18,
   "durationMs": 4.6
  },
  {
   "url": "/~a/shell@1/v4/shell/sync-state.js",
   "startMs": 19,
   "durationMs": 4.6
  },
  {
   "url": "/~a/shell@1/v4/frame/launcher.js",
   "startMs": 19,
   "durationMs": 4.8
  },
  {
   "url": "/~a/shell@1/v4/frame/taskbar.js",
   "startMs": 19,
   "durationMs": 4.9
  },
  {
   "url": "/~a/shell@1/v4/shell/report.js",
   "startMs": 19,
   "durationMs": 5.1
  },
  {
   "url": "/~a/shell@1/v4/frame/topbar.js",
   "startMs": 19,
   "durationMs": 5.1
  },
  {
   "url": "/~a/shell@1/v4/frame/glossary.js",
   "startMs": 19,
   "durationMs": 5.2
  },
  {
   "url": "/~a/shell@1/shared/window-layout-resolver.js",
   "startMs": 23,
   "durationMs": 1.2
  },
  {
   "url": "/~a/shell@1/shared/window-close-gate.js",
   "startMs": 23,
   "durationMs": 1.2
  },
  {
   "url": "/~a/shell@1/v4/shell/column-resizers.js",
   "startMs": 24,
   "durationMs": 1.4
  },
  {
   "url": "/~a/shell@1/v4/frame/window-sheet.js",
   "startMs": 24,
   "durationMs": 1.4
  },
  {
   "url": "/~a/shell@1/shared/context-chat.js",
   "startMs": 24,
   "durationMs": 1.3
  },
  {
   "url": "/~a/shell@1/shared/topbar-apps.js",
   "startMs": 25,
   "durationMs": 0.3
  },
  {
   "url": "/~a/shell@1/shared/pane-grammar.js",
   "startMs": 26,
   "durationMs": 1
  },
  {
   "url": "/~a/shell@1/shared/resizer.js",
   "startMs": 26,
   "durationMs": 0.8
  },
  {
   "url": "/~a/shell@1/v4/frame/pane-heads.js",
   "startMs": 26,
   "durationMs": 1
  },
  {
   "url": "/~a/shell@1/v4/frame/theme/theme.css",
   "startMs": 38,
   "durationMs": 0.7
  },
  {
   "url": "/~a/shell@1/shared/base.css",
   "startMs": 38,
   "durationMs": 0.8
  },
  {
   "url": "/~a/shell@1/shared/shell-v2-chrome.css",
   "startMs": 38,
   "durationMs": 0.7
  },
  {
   "url": "/~a/shell@1/shared/shell-v2-topbar.css",
   "startMs": 38,
   "durationMs": 0.9
  },
  {
   "url": "/~a/shell@1/v4/frame/frame.css",
   "startMs": 38,
   "durationMs": 1
  },
  {
   "url": "/~a/formular@1/index.css",
   "startMs": 169,
   "durationMs": 0.5
  },
  {
   "url": "/~a/formular@1/index.js",
   "startMs": 183,
   "durationMs": 0.4
  },
  {
   "url": "/~a/formular@2/index.css",
   "startMs": 718,
   "durationMs": 3.9
  },
  {
   "url": "/~a/formular@2/index.js",
   "startMs": 723,
   "durationMs": 1.9
  },
  {
   "url": "/~a/notizen@1/index.css",
   "startMs": 1787,
   "durationMs": 2.8
  },
  {
   "url": "/~a/notizen@1/index.js",
   "startMs": 1793,
   "durationMs": 1.7
  },
  {
   "url": "/~a/formular@3/index.css",
   "startMs": 31947,
   "durationMs": 4.1
  },
  {
   "url": "/~a/formular@3/index.js",
   "startMs": 31952,
   "durationMs": 1.8
  },
  {
   "url": "/~a/formular@4/index.css",
   "startMs": 41182,
   "durationMs": 2.1
  },
  {
   "url": "/~a/formular@4/index.js",
   "startMs": 41184,
   "durationMs": 1.2
  },
  {
   "url": "/~a/shell@2/v4/frame/index.js",
   "startMs": 41968,
   "durationMs": 1.4
  },
  {
   "url": "/~a/shell@2/shared/window-manager.js",
   "startMs": 41972,
   "durationMs": 4.9
  },
  {
   "url": "/~a/shell@2/shared/taskbar-pins.js",
   "startMs": 41972,
   "durationMs": 3.6
  },
  {
   "url": "/~a/shell@2/v4/shell/kernel-adapter.js",
   "startMs": 41972,
   "durationMs": 4.1
  },
  {
   "url": "/~a/shell@2/v4/shell/status-zone.js",
   "startMs": 41972,
   "durationMs": 4.7
  },
  {
   "url": "/~a/shell@2/v4/shell/app-host.js",
   "startMs": 41972,
   "durationMs": 4.5
  },
  {
   "url": "/
```

<!-- MESSWERTE:END -->

## Messwerte gegen den Kern-Stub

<!-- STUB:START -->
_Automatisch von measure-stub.mjs, Lauf 2026-10-15T12:18:39.417Z._

### a-stub-remount-zwei-tabs — BESTANDEN

20 Remounts gegen createStubKernel (latencyMs 20): Veröffentlichen (sim.publishAppVersion im Leiter A) → mounted in B p50 10.2 ms / p95 22.1 ms, in A p50 10.2 / p95 21.8 ms; Abweichungen Wert/Fokus/Auswahl/Scroll: 0.

```json
{
 "publishToMountedMs": {
  "tabB_reader": {
   "n": 20,
   "p50": 10.2,
   "p95": 22.1,
   "min": 8.5,
   "max": 28.2
  },
  "tabA_leader": {
   "n": 20,
   "p50": 10.2,
   "p95": 21.8,
   "min": 8.1,
   "max": 28
  }
 },
 "rawB": [
  16.6,
  20.5,
  9.7,
  9.8,
  9.6,
  8.8,
  10.2,
  22.1,
  9.6,
  8.5,
  10.2,
  11.1,
  9.5,
  9.4,
  18.5,
  16.2,
  11.4,
  28.2,
  12.1,
  10.6
 ],
 "mismatches": [],
 "errors": []
}
```

### c-stub-rueckfall — BESTANDEN

v3-Inhalt als Stub-Version 99 → Rückfall auf 21 nach 9022.6 ms (≤ 10 s: true); Zone: „Version 99 startet nicht (mount() nicht binnen 9 s fertig); 21 läuft weiter“; Eingaben erhalten: true; reportFailed zugestellt 2×; Zeiger danach (A/B zurück: true/true): {"activated_ver":"21","target_ver":"99","state":"failed","error":"reported: mount() nicht binnen 9 s fertig"}; kernel.apps activated_ver 21; neues Fenster B 21/mounted, A 21/mounted.

```json
{
 "rollbackMs": 9022.6,
 "zone": {
  "level": "error",
  "text": "Version 99 startet nicht (mount() nicht binnen 9 s fertig); 21 läuft weiter",
  "entries": [
   {
    "level": "error",
    "code": "shell.rollback",
    "text": "Version 99 startet nicht (mount() nicht binnen 9 s fertig); 21 läuft weiter"
   }
  ]
 },
 "activationAfter": {
  "key": "activation:formular",
  "app_id": "formular",
  "activated_ver": "21",
  "target_ver": "99",
  "state": "failed",
  "error": "reported: mount() nicht binnen 9 s fertig",
  "updated_at": 1792066731492,
  "previous_ver": "21",
  "good_vers": [
   "13",
   "14",
   "15",
   "16",
   "17",
   "18",
   "19",
   "20",
   "21"
  ],
  "at_seq": 44
 },
 "pointerBack": {
  "A": true,
  "B": true
 },
 "reports": [
  {
   "appId": "formular",
   "ver": "99",
   "reason": "mount() nicht binnen 9 s fertig",
   "delivered": true,
   "at": 1792066731486,
   "activated_ver": "21"
  },
  {
   "appId": "formular",
   "ver": "99",
   "reason": "mount() nicht binnen 9 s fertig",
   "delivered": true,
   "at": 1792066731489,
   "activated_ver": "21"
  }
 ],
 "newWindow": {
  "B": {
   "ver": "21",
   "state": "mounted"
  },
  "A": {
   "ver": "21",
   "state": "mounted"
  }
 },
 "appRow": {
  "activated_ver": "21",
  "state": "failed",
  "layout": {
   "icon_asset": null,
   "icon_svg": null,
   "category": "Labor",
   "frame_palette": null,
   "default_width": null,
   "default_height": null,
   "min_width": null,
   "min_height": null,
   "presentation": null
  },
  "styles": [
   "index.css"
  ]
 },
 "errors": []
}
```

<!-- STUB:END -->
