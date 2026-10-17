# Strang Q – Quelltext-Modus der Shell (Stand 15.10.2026, 13:40)

Owner-Vorgaben (15.10., wörtlich): Apps on the fly hochladen/editieren/ändern, Änderungen in Echtzeit synchronisiert,
ohne Backend-Recompile; Hot-Plug-and-Play inklusive Anlegen und Editieren der Collections; Code jeder App direkt in der
App sichtbar, aber nur für Admins. Grundlage: Entwurf §5/§7/§8, Architektur §9.1, §9.2, §10.2a Nr. 7, §16.4, §16.10.

## Ergebnis

Abnahme `node src/apps/business-os/v4/source/lab/accept.mjs` (Monaco aus `vendor/monaco`) und
`… --editor textarea` (Rückfall-Editor): **beide 7/7 bestanden** (`lab/results.json`, `lab/results-textarea.json`),
gegen den eingefrorenen Stub `~/.local/state/shell-v4/stub-pins/66edb98` (überschreibbar per `CTOX_APP_API_STUB`).
Einheitstests `node --test src/apps/business-os/v4/source/schema-diff.test.mjs`: 11/11. Wächter
`v4/tools/guards/guard-v4.mjs`: 0 Treffer unter `v4/source/`.

| Nr. | Abnahme | Beleg (Lauf mit Monaco) |
|---|---|---|
| 1 | Nicht-Admin sieht keinen Umschalter | Tab C (`nina`, Rolle user, Rechte leer): Knopf `hidden`, Admin-Tab sichtbar. `shots/1-nonadmin-*.png` |
| 2 | Admin ändert Datei per Tastatur, veröffentlicht → zweiter Tab montiert live | `label = 'Version zwei';` in Monaco getippt; `1 → s4`; Tab B zeigt „Version zwei“ (~45–110 ms nach Annahme); Editor bleibt offen auf Basis s4. `shots/2-editor-*.png` |
| 3 | Optionales Feld mit Default → „kompatibel“, keine Migration | Diff `items: compatible`, keine Sperre, veröffentlicht `s6`, Generation bleibt 1, Feld im Manifest. `shots/3-compatible-*.png` |
| 4 | Feldtyp ändern → „neue Generation“, ohne migrate_fn gesperrt, mit geht es | Grund „Typ von „count“: number → string“; Knopf gesperrt **und** Core-Attrappe lehnt direkten Aufruf ab (`invalid`, „Migration fehlt: items“); „Migration anlegen“ legt `migrations/items.js` + Eintrag `{from_ver:'s6'}` an; danach `s8`, Generation 2, Bestandsdokument migriert (`count: '3'`). `shots/4-generation-*.png` |
| 5 | Zwei Admins, gleiche Basis → zweiter bekommt conflict, Entwurf bleibt | ada und bert beide auf `s8`; ada → `s11`; bert → Zustandszone „Veröffentlichen abgelehnt: Basis s8 ist nicht mehr aktiv (aktiv: s11). Dein Entwurf bleibt erhalten.“; Entwurf (Kern) und Editortext unverändert; „Basis aktualisieren“ erscheint. `shots/5-conflict-*.png` |
| 6 | App-Upload neuer App → im Launcher-Datenstrom (kernel.apps) | Ordner `lab/apps/zettel` über `input[type=file][webkitdirectory]`; App-ID aus `manifest.json`; Tab B sieht `zettel@s13` aktiviert und montiert sie. `shots/6-upload-*.png` |

Screenshots je Schritt in 1280 und 420 px angesehen. Behobene Layoutfehler: Dateiauswahl war bei 420 px auf 0 px
gequetscht (Leiste bricht jetzt um, Auswahl volle Zeile); Diff-Fläche nahm fest 45 % ein und ließ Leerraum (jetzt
Inhaltshöhe, höchstens 45 %); doppelter App-Titel (Fensterkopf + Leiste) entfernt.

## Bausteine (alle unter `v4/source/`)

- `source-mode.js` – `openSourceMode({ root, appRecord, kernel, api, zone?, editor? }) → Promise<{ close(), el, flush(), editor, state }>`.
  Dateibaum aus der Version (Gruppen App: Manifest/Collections · Handler · Migrationen · Dateien; geänderte Dateien mit
  Punkt, ohne Zähler), Editor, Collections-Ansicht (Editor + Diff je Collection, Pflicht-Migration), Manifest-Formular
  (Titel, Einstieg, App-API, Fähigkeiten, Handler). Schreiben entprellt über `source.draft.write`, Collections zusätzlich
  über `source.draft.setCollections({ base_app_ver, file, target, changes[] })`. Veröffentlichen =
  `draft.publish({ base_app_ver })`; `stale_base`/`busy` → `source.conflict` in die Zustandszone, sonstige Ablehnung →
  `source.publish`; ungültiges Schema → `source.schema`. Ohne `apps.source.edit` nur lesbar, Verlust von
  `apps.source.view` schließt die Ansicht. Neue aktive Version ohne eigenen Entwurf: Ansicht folgt still; mit Entwurf:
  Basis bleibt, „Basis aktualisieren“ setzt den Entwurf auf die aktive Version.
- `schema-diff.js` – reine Einstufung nach §16.4 (streng nach Owner: kompatibel nur neues optionales Feld **mit** Default,
  Index, Rechte, Klasse/Limit/Budget, neue Collection; alles andere neue Generation). Migration gilt nur mit
  `from_ver === base_app_ver` und vorhandener Datei (alter Eintrag einer früheren Generation zählt nicht).
- `editor.js` – Monaco aus dem lokalen `vendor/monaco` (Basis über `resolveMonacoBaseUrl` aus
  `desktop-apps/code-editor/app.js`), 6-s-Frist, sonst textarea mit Zeilennummern.
- `upload.js` – `readUploadedFiles`, `createFolderInput` (multiple + webkitdirectory, ohne ZIP), `publishUpload`,
  `openAppUpload({ root, kernel, api, zone })` für „neue App“ (neue App: `base_app_ver = null`; bestehende: neue Version).
  In der Quelltext-Ansicht lädt „Ordner…“ einen Ordner in den Entwurf der offenen App.
- `chrome-toggle.js` – `attachSourceToggle({ controls, content, zone, appRecord, kernel, api, editor })`: Umschalter
  `</>` vorn in die Fenstersteuerung, sichtbar nur mit `apps.source.view`; die Ansicht liegt im Fenster direkt hinter dem
  Inhaltsbereich, der App-Inhalt bleibt montiert (Live-Remount läuft darunter) und wird nur ausgeblendet.
- `source-mode.css` – dicht, Shell-Tokens (`--v4-*`), Container-Query ≤ 560 px Fensterbreite, Dunkelmodus.

## Hinweise für den Integrations-Strang (Rahmen)

1. Der Rahmen wurde während dieses Strangs umgebaut (`v4/shell/shell.js` und `window-chrome.css` gelöscht,
   `v4/frame/index.js` ist jetzt „die eine Shell“, Fenster aus `shared/window-manager.js`). Deshalb hängt
   `attachSourceToggle` nur an drei Elementen (Steuerung, Inhaltsbereich, Zone) statt an Chrome-Klassen. Das Labor nutzt
   ein eigenes Minimal-Chrome (`lab/lab-window.js`) und `kernel.shell.mount` des Stubs für Live-Remount.
2. `api` muss ein **stabiler** actor-Spiegel sein (z. B. `kernel.appApi(appId, 'shell-mirror')`), nicht die
   fensterbezogene App-API: deren Abonnements entsorgt der App-Host beim Remount, der Umschalter würde verstummen.
3. `zone` = die Zustandszone des Fensters (`{report, clear}`); Codes `source.conflict`, `source.publish`,
   `source.schema`, `source.draft`, `source.open`, `source.upload`. Diese Texte gehen ausschließlich in die Zone.
4. `kernel` = Shell-Kernel-API mit `source(appId)`, `apps`, `artifacts.manifest`. Bis „ctox sync v4“ die Lücken unten
   schließt: `lab/source-attrappe.js` vor den Stub schalten.

## Abweichungen und Vertragslücken (Stub-Pin 66edb98 → Vorschläge an „ctox sync v4“)

`source.*` existiert im Stub, reicht aber für §9.2/§16.4 nicht. `lab/source-attrappe.js` hält den Vertrag `SourceApi` und füllt:

- **A1 Entwurf je Nutzer.** Der Stub legt den Entwurf je App in `meta` ab (`draft:<app>`): zwei Admins im selben
  Tab-Verbund überschreiben sich, und `publish` des einen leert den Entwurf des anderen – Abnahme 5 wäre damit nicht
  erfüllbar. Attrappe: eigener Speicher je `(actor.id, app_id)`.
- **A2 Core-Prüfung beim Veröffentlichen.** Der Stub prüft weder Schema-Diff noch Migrationen und reicht beim
  Entwurfsweg kein `incompatible`/`migrate` an `publishInternal` weiter (keine neue Generation). Attrappe: Diff +
  Pflicht-Migration (Ablehnung `invalid`), dann `sim.publishAppVersion(…, { incompatible, migrate, baseAppVer })`.
  `migrate` wirkt nur im Leiter-Tab (Funktionen sind im Stub nicht per BroadcastChannel übertragbar).
- **A3 Neue App** (Upload): Entwurfsweg mit `base_app_ver = null`.
- **A4 `draft.info()` / `draft.discard()` / `draft.rebase(ver)`** – nicht im Vertrag. Ohne `info()` kennt die Shell nach
  Neuladen die Basis eines Entwurfs nicht; ohne `discard()` kann der Nutzer einen Entwurf nicht verwerfen. Die Shell
  nutzt sie, wenn vorhanden, sonst Basis = aktive Version.
- **A5 Manifest-Felder** kommen aus der Entwurfsdatei `manifest.json` (Vertrag hat kein `draft.setManifest`).
- **A6 Form von `setCollections(schemaDiff)`** ist im Vertrag offen; gesendet wird
  `{ base_app_ver, file: 'collections.schema.json', target, changes: [{ name, kind, reasons, migrate_fn }] }`.
- **A7 Dateien löschen / umbenennen** im Entwurf: kein `draft.remove(path)` im Vertrag; nicht gebaut.
- **A8 Versionsname** im Labor `s<coreHead+1>` (wie der Stub-Entwurfsweg).
- **A9 Format `collections.schema.json`**: gelesen werden `{collections: {name: Schema}}` (heutige Module) und
  `{collections: {name: {schema, indexes, class, rights, client_limit, byte_budget}}}`; Migrationen als
  `migrations: {name: {from_ver, migrate_fn}}` (§9.1). Die Altform `migration_strategies` wird nicht ausgewertet.

## Nicht erledigt / offen

- Integration in `v4/frame` (anderer Strang, siehe Hinweise oben).
- Abnahme 2 tippt echt (Tastatur in Monaco/textarea). Abnahmen 3–5 ersetzen den ganzen Dateitext über
  `handle.editor.replaceAll` (wie Einfügen; löst denselben onChange-Weg aus wie Tippen), weil JSON-Tippen in Monaco
  durch Auto-Schließen von Klammern unzuverlässig ist.
- Nicht gegen einen echten Core geprüft (es gibt noch keinen); alle Belege stammen aus Stub-Pin + Attrappe.
- Kein git (Vorgabe). Eingerichtete Dateien liegen uncommittet im Klon `~/Documents/ctox-shell-v4` (Zweig shell-v4/prototype).

---

# Nachtrag Q2 – Umstellung auf die Quelltext-API des Kerns (Stub-Pin cff5ba5, §16.22 A1–A9), 15.10.2026

Der Quelltext-Modus nutzt jetzt ausschließlich `kernel.source(appId)` des Stubs. `lab/source-attrappe.js` ist
**gelöscht** (keine Stub-Lücke mehr, die eine Attrappe bräuchte). Labor: `server.mjs` liefert Pin cff5ba5 (Default,
überschreibbar per `CTOX_APP_API_STUB`) inkl. `/stub/testkit/`; Apps montiert `testkit/app-host.mjs`
(`createAppHost(kernel).mount`), weil `kernel.shell.mount` seit 67229d2 nicht mehr im Kernel liegt.
Damit überholt: der Abschnitt „Abweichungen und Vertragslücken (Stub-Pin 66edb98)“ oben (A1–A9 sind im Kern gelöst),
die Form `setCollections({ base_app_ver, file, target, changes })`, der Knopf „Basis aktualisieren“ und Hinweis 4.

## Ergebnis

`node --test v4/source/schema-diff.test.mjs`: **13/13** (neu: `steps`-Migration, Gleichlauf-Test gegen
`classifyCollections` des Stubs mit 12 Varianten). `node v4/source/lab/accept.mjs` (Monaco) und `… --editor textarea`:
**beide 11/11** (seed + Schritte 1–6, 8–11; `lab/results.json`, `lab/results-textarea.json`, Fehlerliste leer).
Wächter `guard-v4.mjs`: 0 Treffer unter `v4/source/`.

| Nr. | Abnahme | Beleg (Lauf mit Monaco) |
|---|---|---|
| 1–2 | wie oben | unverändert bestanden; Tab B montiert `s5` live (~95 ms) |
| 3 | kompatibel | Diff `items: compatible`, **Quelle Kern** (`data-source="kernel"`), 0 Abweichungen zur Shell-Einstufung |
| 4 | neue Generation | Kern-Diff, Knopf gesperrt; direkter Kernaufruf → `rejected{invalid, „Migration fehlt: items“}`; mit Migration Generation 2, `count: '3'` |
| 5 | Zwei Admins, stale_base | bert → Zone „Veröffentlichen abgelehnt: Basis s16 ist nicht mehr aktiv (aktiv: s22) …“ **mit Aktion „Auf aktuelle Version setzen“**; Entwurf (Kern) und Editortext unverändert. `shots/5-conflict-*.png` |
| 8 | getrennte Entwürfe | Zeilen `app_drafts['ada/demo']` und `['bert/demo']` nebeneinander, jeder `draft.info()` zeigt nur den eigenen Text; nach adas publish ist nur `ada/demo` weg, `bert/demo` bleibt |
| 9 | rebase + discard | Zonen-Aktion → `draft.rebase(s22)`: Basis s22, bert-Text bleibt, Zone leer (`shots/9-rebase-*.png`); „Verwerfen“ → `draft.discard()`, `info()` = null, Editor zeigt die aktive Fassung (ada) |
| 10 | Löschen / Umbenennen im Dateibaum | Umbenennen auf vorhandenen Pfad → Zone „„index.css“ nicht umbenannt: handlers/recount.js existiert bereits“; `handlers/recount.js → handlers/zaehlen.js` (Inline-Eingabe am Eintrag, Enter), `index.css` gelöscht; Entwurf `files: {recount: null, index.css: null, zaehlen: text}`, Handler-Verweis per `setManifest` mitgezogen; veröffentlichte Version ohne beide Dateien, Handler zeigt auf `zaehlen.js`, Tab B remountet. `shots/10-files-*.png` |
| 11 | setManifest mit Schutzfeld | „Weitere Felder“ `{ "app_id": "anders" }` → Zone „Manifest nicht übernommen: Manifest-Feld app_id ist nicht änderbar“, Entwurf ohne `app_id`; direkter Kernaufruf mit `generations` → `invalid`. Danach `triggers` + Titel „Demo (neu)“ → `draft.manifest`, **keine** Datei `manifest.json` im Entwurf; veröffentlicht mit `triggers`, `app_id: demo`, Handler weiter `zaehlen.js`, Titel in Tab B. `shots/11-manifest-protected-*.png` |
| 6 | Upload neue App | `publishUpload`: discard → fehlende Basisdateien `draft.remove` → Dateien `write`, Schema `setCollections` → `publish({ base_app_ver: null })`; `zettel` aktiviert und in Tab B montiert |

Screenshots 1280/420 angesehen (5, 9, 10, 11, 4). Bei 420 px stehen ✎/× für die aktuelle Datei neben der Dateiauswahl,
die Knöpfe brechen in die zweite Zeile um; die Zonen-Aktion sitzt rechts in der Zonenzeile, der Text kürzt sich davor.

## Was die Shell jetzt tut (source-mode.js / upload.js)

- Öffnen und jede Kernaktion: `draft.info()` → Basis (`base_app_ver`, auch `null`), Dateien (`null` = gelöscht),
  Manifest-Felder. Ansicht = `list(base)` + Entwurf; Nur-Lesen ohne `apps.source.edit` ruft keine `draft.*` auf.
- Dateien: `draft.write` (entprellt 250 ms), `draft.remove`, `draft.rename` (Aktionen ✎/× je Baumzeile, Umbenennen
  inline am Eintrag). Verweist `entry` oder ein Handler auf die umbenannte Datei, geht die Korrektur per `setManifest`.
- Collections: Schematext → `draft.setCollections(text)`; der zurückgegebene Diff ist maßgeblich
  (`state.diffSource = 'kernel'`), Sperre aus `publishBlockers(kernDiff, Pfade)`. Ungültiges JSON: Kern lehnt
  `invalid` ab → Zone `source.schema`, Text wird trotzdem per `draft.write` gehalten. Beim Tippen zeigt die lokale
  Einstufung sofort eine Vorschau (`diffSource = 'local'`), bis der Kern antwortet.
- Manifest: Formular (Titel, Einstieg, App-API, Fähigkeiten, Handler) plus „Weitere Felder“ (JSON-Objekt, z. B.
  `triggers`, `command_rights`, `layout`) → `draft.setManifest(patch)`; Ablehnung → Zone `source.manifest`.
- Veröffentlichen: `draft.publish({ base_app_ver })`. `stale_base` → Zone `source.conflict` mit
  `action: { label: 'Auf aktuelle Version setzen', run }` = `draft.rebase(aktiv)`; `busy` ohne Aktion; sonst
  `source.publish` mit `detail`. „Verwerfen“ = `draft.discard()`.
- `handle.state`: `base, draft, deleted, paths, manifest, blockers, diff, diffSource, deviations`.

## Abweichungen Shell-Diff ↔ Kern-Diff (Protokoll)

Jeder Kern-Diff wird mit `schema-diff.js` verglichen; Abweichungen landen in `state.deviations` und als
`console.warn('[v4/source] Diff-Abweichung …')`. In beiden Abnahmeläufen: **keine**. Vorab gefunden und behoben:

1. Kern akzeptiert Migrationen mit deklarativen `steps` (ohne `migrate_fn`), `schema-diff.js` nicht → `classifyCollections`
   und `publishBlockers` übernehmen das (Test „Gleichlauf mit Kern-Stub“).
2. Basis ohne `collections.schema.json`: der Kern vergleicht gegen `manifest.collections` der Basis, die Shell
   verglich gegen „leer“ → Shell nimmt jetzt ebenfalls das Basis-Manifest.

Bekannt, ohne Wirkung auf die Abnahme: Der Kern lehnt `collections`/`migrations`, die keine Objekte sind, mit
`invalid` ab; `parseCollectionsFile` der Shell prüft das nicht (die Kern-Ablehnung landet in der Zone).

## Neue Vertragslücken (Vorschläge an „ctox sync v4“)

- **B1 Veraltete `manifest.json` schlägt das veröffentlichte Manifest.** `publishDraft` legt `manifest.json` (aus der
  Basis übernommen) über das Basis-Manifest. Nach einem Veröffentlichen mit `setManifest` (z. B. Handler umgezogen) ist
  die Datei veraltet, und das nächste Veröffentlichen ohne erneutes `setManifest` setzt den alten Wert zurück – in der
  Abnahme zeigte der Handler danach auf die gelöschte `handlers/recount.js`. Shell-Gegenmaßnahme: Ist die Basis über
  `draft.publish` entstanden (`provenance.locally_modified`) und `manifest.json` im Entwurf unverändert, schreibt die
  Shell abweichende Felder des Basis-Manifests vor dem Veröffentlichen per `setManifest` fest. Rest-Risiko: Wird der
  Entwurf danach auf eine neuere Basis gesetzt, gelten diese Festschreibungen weiter. Vorschlag: `publishDraft`
  schreibt `manifest.json` aus dem wirksamen Manifest neu, oder ein unverändertes `manifest.json` rangiert unter dem
  Basis-Manifest.
- **B2 Aktion an Zonen-Einträgen.** `StatusItem` hat kein Feld für eine Aktion. Die Shell meldet `stale_base` mit
  `action: { label, run }` an die Fenster-Zone (`zone.report`); das Labor-Chrome rendert sie. `v4/shell/status-zone.js`
  (anderer Strang) muss `item.action` als Knopf in der Zonenzeile rendern, sonst fehlt nach `stale_base` der Weg zu
  `rebase`. Über `kernel.status` (Strom) ist eine Funktion nicht transportierbar – die Aktion bleibt fensterlokal.
- **B3 Löschen eines Felds aus `manifest.json` per `setManifest`** ist nicht möglich: `null` entfernt das Feld nur
  aus `draft.manifest`, `manifest.json` liefert es weiter. „Weitere Felder“ leeren wirkt deshalb nur auf Felder, die
  über `setManifest` kamen.

## Offen

- Integration der Zonen-Aktion in `v4/shell/status-zone.js` und des Umschalters in `v4/frame` (andere Stränge).
- Keine Live-Übernahme des Entwurfs, wenn derselbe Nutzer ihn in einem zweiten Tab ändert (Ansicht liest beim Öffnen
  und nach eigenen Aktionen).
- Kein git (Vorgabe); alle Änderungen liegen uncommittet im Klon.

---

# Nachtrag Q3 – Kern-Stub 9fa9948 (§16.26, Befund B1 behoben), 15.10.2026

Labor-Default jetzt `~/.local/state/shell-v4/stub-pins/9fa9948` (`lab/server.mjs`, `schema-diff.test.mjs`; weiter
überschreibbar per `CTOX_APP_API_STUB`). Die Shell-Notlösung für B1 ist **entfernt**: kein `manifestPins()`, kein
`baseAuthoritative()`, kein Lesen einer Entwurfsdatei `manifest.json`, kein Festschreiben von Basis-Manifestwerten per
`setManifest` vor dem Veröffentlichen, kein Mitziehen von Verweisen durch die Shell beim Umbenennen.

## Ergebnis

`node --test v4/source/schema-diff.test.mjs`: **13/13** (Gleichlauf gegen `classifyCollections` aus 9fa9948).
`node v4/source/lab/accept.mjs` (Monaco) und `… --editor textarea`: **beide 13/13** (seed, 1–6, 8–11 wie bisher, neu
12 und 13; `lab/results.json`, `lab/results-textarea.json`, Fehlerliste leer). Wächter `guard-v4.mjs`: Exit 0.

| Nr. | Abnahme | Beleg (Lauf mit Monaco) |
|---|---|---|
| 2 (erweitert) | manifest.json nicht als Datei angeboten | Basis `1` enthält `manifest.json`; der Kern trägt sie im Entwurf als `null` ein (`draftFiles: {manifest.json: null, index.js}`); Dateibaum, Dateiauswahl, `state.paths/draft/deleted` ohne `manifest.json` |
| 10 | Umbenennen zieht Handler mit | jetzt durch den Kern: Entwurfsmanifest `{handlers: {demo.recount: handlers/zaehlen.js}}` ohne Shell-`setManifest` |
| 12 | Umbenannter Handler bleibt über zwei Veröffentlichungen verdrahtet | `zaehlen.js → neu.js` im Dateibaum; Entwurf `handlers` vom Kern mitgezogen, Formular zeigt `demo.recount = handlers/neu.js`; veröffentlicht `s37` → `neu.js`; danach nur `index.js` geändert (Entwurfsmanifest `{}`), veröffentlicht `s41` → Handler weiter `neu.js`, Dateien ohne `zaehlen.js` und ohne `manifest.json`; Tab B montiert „Version vier“. `shots/12-handler-republish-*.png` |
| 13 | Verweis auf gelöschte Datei | `handlers/neu.js` per × gelöscht, Veröffentlichen → Zone „Veröffentlichen abgelehnt (invalid): Verweis auf fehlende Datei: handlers.demo.recount → handlers/neu.js. Dein Entwurf bleibt erhalten.“; Entwurf (Basis s41, Datei `null`) und aktive Version unverändert. Danach Handler im Formular geleert → `setManifest({handlers: {demo.recount: null}})` entfernt den Basis-Handler, veröffentlicht `s46` mit `handlers: {}`, Zone leer. `shots/13-dangling-ref-*.png` |
| 6 (erweitert) | Upload mit manifest.json | `manifest.json` des Ordners → `setManifest` (ohne Kernfelder), veröffentlichte `zettel`-Version ohne `manifest.json`, Titel „Zettel“ |

Screenshots 12/13 in 1280 und 420 px angesehen; bei 420 px kürzt die Zonenzeile den Text, die volle Ablehnung steht
in der aufklappbaren Liste.

## Was sich in der Shell geändert hat

- `source-mode.js`: `manifest.json` ist nie Teil von `livePaths()`/`s.draft` (auch nicht als gelöscht).
  Wirksames Manifest = Basis-Manifest + `draft.info().manifest` + noch nicht gesendete Eingaben, mit den Patch-Regeln
  des Kerns (`handlers` gegen die wirksame Karte, `null` entfernt auch Basis-Handler; zwei Handler-Patches in der
  Entprellung werden zusammengesetzt statt ersetzt). Nach `setManifest` und `rename` liest die Shell das
  Entwurfsmanifest aus `draft.info()` zurück, statt es selbst nachzurechnen. Eine `manifest.json` aus „Ordner…“ wird zu
  `setManifest` (Handler als ganze Karte). Publish-Ablehnungen `invalid` gehen mit `detail` und „Dein Entwurf bleibt
  erhalten.“ in die Zone (`source.publish`); die Shell sperrt fehlende Verweise nicht selbst – der Kern ist maßgeblich.
- `upload.js` (`publishUpload`): `manifest.json` wird nicht geschrieben und nicht entfernt, sondern per `setManifest`
  übernommen (Kernfelder `app_id`, `collections`, … weggelassen; Basis-Handler, die der Ordner nicht nennt = `null`).
- Labor: `lab.js` veröffentlicht die Seed-Version mit den Feldern aus `manifest.json` im Versions-Manifest (wie ein
  Release; vorher kamen die Handler nur über die Datei); `accept.mjs` Schritte 12, 13 und Erweiterungen von 2 und 6.

## Neuer Befund an „ctox sync v4“

- **B4 Zusatzfelder erben die Basis nicht.** `publishDraft` übernimmt `manifestExtra` (z. B. `triggers`,
  `command_rights`) nur aus `draft.manifest`, nicht aus dem Basis-Manifest – entgegen „fehlende erbt die Basis“
  (CHANGES 9fa9948). Beleg Abnahme 12: `s33` (Schritt 11) hat `triggers`, das nächste Veröffentlichen (`s37`,
  Entwurfsmanifest nur mit den vom Kern mitgezogenen `handlers`) und `s41` (Entwurfsmanifest `{}`) nicht mehr (`results.json`, `12-…`.data.triggers). Die Shell zeigt das wirksame
  Manifest nach Vertrag (Basis + Entwurf) und schreibt bewusst nichts fest (keine neue Notlösung). Vorschlag:
  `publishDraft` startet die Zusatzfelder aus dem Basis-Manifest und legt `draft.manifest` darüber (`null` entfernt).
- B2 (Aktion an Zonen-Einträgen) bleibt offen; B3 ist mit §16.26 erledigt (`manifest.json` gibt es im Entwurf nicht mehr).

## Offen

- Wie im Nachtrag Q2: Integration in `v4/shell/status-zone.js`/`v4/frame` (andere Stränge), keine Live-Übernahme des
  Entwurfs aus einem zweiten Tab desselben Nutzers. Kein git (Vorgabe); alle Änderungen liegen uncommittet im Klon.
