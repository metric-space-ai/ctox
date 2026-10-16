# creator – Port auf App-API 4.0 (Strang UC2, Welle C)

Stub-Pin cff5ba5 (Erstport gegen 67229d2). Start 14:20, fertig 14:24 (Berlin, gemessen).

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app creator` | 36 → 0 Stellen, Exit 0 (CSS vorher 94 ungescoped, 10 mit Shell-Vorfahr) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs creator modules/creator/index.css --check` | gescoped |
| Wächter | `node v4/tools/guards/guard-v4.mjs --app modules/creator` | 21 → 0 Treffer |
| v4-Test | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/cff5ba5 node --test src/apps/business-os/modules/creator/tests/v4-api.test.mjs` | 10/10 grün |
| Legacy-Suite | `node modules/creator/creator.test.mjs` (aus Kopie mit node_modules-Link) | 23/23 grün |
| Screenshots | `tests/shots/creator-1280.png`, `creator-420.png` | angesehen: Bibliothek + Composer bzw. gestapelt (Composer zuerst), kein Überlauf |

v4-Test deckt ab: mount (Eigenbau-Apps aus `business_apps`, Kern-Apps ausgefiltert, Titel, kein Status-/Protokollfeld,
keine Zone) · Katalog-Änderung live, Löschung verschwindet, Aufträge-Band aus `business_commands` · leere
Collections: kein Text, keine Zone · „App erstellen“: `ctox.business_os.app.create` ohne Warten (Knopf bleibt
bedienbar), `failCommand` → genau ein Zoneneintrag, Wiederholung `done` räumt ihn · `api.put` → `accepted`, live im
Aufträge-Band · Datentabelle entfernen fragt über `api.shell.dialog` · App wählen füllt den Auftrag aus dem Katalog
(ohne HTTP) · snapshot → Remount erhält Auftragstext, App-ID, Datentabellen, Auswahl, Knopfzustand · unmount löst
alle Abos und leert root.

## Entscheidungen

- **App-Anlage bleibt ein Befehl**: `ctox.business_os.app.create` (Core-generisch, Agent baut die App). Kein eigener
  Installationsweg. `api.command(type, {...payload, module:'creator', record_id, client_context}, {record_id})`, nie
  warten (vorher `commandBus.dispatch(..., {until:'terminal'})` mit gesperrtem Knopf, Sync-Punkt und Benachrichtigung).
  Der Auftrag erscheint live unter „Aufträge“; Fehler → Zone `creator.command.ctox.business_os.app.create`.
  `buildAppCreateCommand` liefert keine `command_id` mehr (vergibt `api.command`).
- **Katalog**: `business_module_catalog`-Dokument `module-catalog` → `api.query('business_apps')` (je App ein
  Dokument, §16.14); `normalizeCreatorInstalledApps({modules: rows})` mit unveränderter Filterregel (nur Eigenbau-Apps:
  `source === 'installed'` bzw. `entry` unter `installed-modules/`). Aufträge: `api.query('business_commands')`.
- **„App ändern“-Vorbelegung**: der HTTP-Abruf `installed-modules/<id>/module.json` entfällt (R10); es gilt der bisherige
  Rückfall aus dem Katalogeintrag. Hash-Einstieg `#creator?upgrade=<id>` / `ctx.args.upgrade` →
  `api.shell.onRoute(ev => ev.params.upgrade)`.
- **Zustandstexte entfernt**: Sync-Punkt + Diagnosezeile im Fuß („Beschreibe zuerst kurz deine App.“, „Bereit …“,
  „CTOX App-Auftrag läuft.“, „Lege CTOX-Auftrag an…“, „App erstellt“, „Fehler beim Speichern“), Protokollfeld
  „Technischer Status“ (`#console-logs-container`, griff per `document.querySelector` außerhalb der App), Bibliotheks-
  Leer-/Sync-/Filtertexte, alle `ctx.notifications`. Warnungen/Fehler → Zone (`creator.request.missing`,
  `creator.request.invalid`, `creator.inspiration.invalid`, `creator.upgrade.unknown`, sonst `creator.<warning|error>`),
  Info-Zeilen nur Konsole. Der Sperrgrund des Knopfs steht nur noch in dessen `title`. `computeCreatorActionState`
  liefert für „läuft“/„bereit“ keinen Text mehr. CSS der entfernten Elemente gelöscht.
- **Dialog**: `showBusinessConfirm` → `api.shell.dialog({kind:'confirm', …, default:false})`.
- **Sonst**: Locales per JSON-Modul-Import + `api.shell.t('creator.<key>')`, Locale live; 14 ungenutzte Schlüssel
  entfernt; Archetyp-Beschriftung „Warteschlange & Workflow“ → „Queue & Workflow“, Beispiel-Prompt (en) „waiting“ →
  „on hold“ (Wächter-Vokabular); `ctx.getActionIcon` entfällt; `autoWirePaneGrammar(root)`; Titel `setTitle`;
  root ist `container: business-app-window`.
- **schema.js gelöscht**: `business_commands` v2, Felder/required gleich `collections.schema.json`, Migration 1
  (`inbound_channel` ← `inbound_channel || module`) steht dort deklarativ als `set_from_first_truthy`, 2 leer.

## Quelltext-Schnittstelle, die creator bräuchte (v4/source/, nur gelesen)

Strang Q baut den Quelltext-Modus **auf der Shell-Kernel-Seite**: `openSourceMode({root, appRecord, kernel, api})`,
`openAppUpload({root, kernel, api})`, `publishUpload(kernel, {appId, files})` – alle über
`kernel.source(appId): SourceApi` (`list`, `read`, `draft.write`, `draft.setCollections`, `draft.publish({base_app_ver})`,
Rechte `apps.source.view`/`apps.source.edit`, Stub `index.d.ts:267`). Apps bekommen in `mount()` nur die App-API, kein
`kernel`. creator baut deshalb keinen eigenen Installationsweg und nutzt weiter den Befehl. Für die zwei Stellen, an
denen creator heute mehr wollte, braucht es (Vorschlag, nicht im Vertrag):

1. **Lesen fremder App-Quellen** (Vorbelegung „App ändern“ aus `manifest.json`/`module.json` statt Katalog-Rückfall):
   App-API `api.apps.source(appId).read(path, ver?) → Promise<string>` und `.list(ver?)`, gleiche Semantik wie
   `SourceApi.read/list`, Recht `apps.source.view`, Fehler `forbidden` ohne Recht.
2. **Direkt anlegen statt Agent-Auftrag** (optional): `api.shell.openSource({ app_id, mode: 'new' | 'edit' })` öffnet die
   Upload-/Quelltext-Fläche der Shell **im creator-Fenster** (`openAppUpload`/`openSourceMode` mit dem App-root), die
   Veröffentlichung bleibt bei der Shell (`draft.publish`). Alternativ ein Befehl
   `ctox.business_os.app.source.publish {app_id, base_app_ver, files}` mit Core-Prüfung wie `draft.publish`.

## Collections (für den Manifest-Generator)

Eigene laut `collections.schema.json`: `business_commands` (Generator-Vorschlag: System-Collection der Shell – dann
entfällt sie hier). Gelesen: `business_apps` (System, §16.14), `business_commands`.

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust-Treffer | Einstufung |
|---|---:|---|
| `ctox.business_os.app.create` | 105 | Core-generisch (App-Erzeugung = Plattform) |
| `ctox.business_os.app.modify` (nur gelesen, Aufträge-Band) | 26 | Core-generisch |
| `business_os.chat.task` (nur gelesen, Aufträge-Band) | 154 | Core-generisch |

## OFFEN

1. **Quelltext-Schnittstelle** für Apps (siehe oben, Punkt 1/2) – Entscheidung Kern/Strang Q.
2. **Katalogfelder**: die Eigenbau-Filterregel braucht in `business_apps` ein Herkunftsmerkmal (`source: 'installed'`
   oder `entry`); der Stub-Katalog trägt nur `id, title, active_ver`. Ohne Merkmal zeigt das Apps-Band nichts.
3. **Aufträge-Erkennung**: `normalizeCreatorRequestSuggestions` erkennt v4-Befehle über `type` + `payload.module` (bzw.
   `command.client_context.source`; v4-Dokumente haben kein `command_type`/`module` auf oberster Ebene) – im Test belegt.
4. **Mehrere Fenster**: Modulzustand modulweit (wie knowledge OFFEN-4).
5. Kein git benutzt.

## Nachprüfung gegen Stub-Pin cff5ba5 (Wiederaufnahme 15.10., bis 15:10 Berlin, gemessen)

Nach dem Sitzungsabbruch ~14:40: keine Datei im Schreibbereich nach 14:25 geändert, keine halben Edits. Unverändert
gegen den neuen Pin neu gefahren:

- `scan.mjs --app creator`: 0 Stellen, CSS ungescoped 0, Exit 0; `scope-css.mjs … --check`: gescoped.
- `guard-v4.mjs --json`: 0 Treffer unter `modules/creator/` (nur getrennt gezählte Testdatei-Treffer).
- v4-Test gegen cff5ba5: 10/10 grün (mit `CTOX_APP_API_STUB` und ohne – Rückfallpfad im Test jetzt cff5ba5).
- Legacy-Suite (Kopie mit node_modules-Link): 23/23 grün.
- Screenshots 1280/420 vom Testlauf neu geschrieben und angesehen: unverändert gegenüber Erstport, kein Überlauf.
