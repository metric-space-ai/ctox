# app-store – Port auf App-API 4.0 (Strang UD6, Welle D)

Status: **abgeschlossen** (15.10.2026). Stub-Pin `~/.local/state/shell-v4/stub-pins/9fa9948` (App-API unverändert
gegenüber cff5ba5/67229d2). Alle 16 Befehlstypen sind **PLATTFORM** (Kern, `V4-HANDLERS.md`, Strang H8); kein Handler.
Ein früherer Lauf brach am 15.10. ~14:40 mitten im Port ab; dieser Lauf hat den Zwischenstand repariert und abgeschlossen.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app app-store` | **0 Stellen**, Exit 0 (Baseline 72 Stellen, 173 Selektoren ungescoped) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs app-store modules/app-store/index.css --check` | gescoped |
| guard-v4 | `node v4/tools/guards/guard-v4.mjs --json` | **0 Treffer** in `modules/app-store/` (Code und Tests) |
| v4-Test | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/9fa9948 node --test src/apps/business-os/modules/app-store/tests/v4-api.test.mjs` | **11/11 grün**, zweimal hintereinander |
| Bestehende Suite | `node --test app-store.test.mjs` / `app-store-mount.test.mjs` / `app-store-shelf-lifecycle.test.mjs` (im Modulordner) | 28/28, 3/3, 3/3 grün (auf v4 umgestellt, nicht gelöscht) |
| WebGL-Regal | `CTOX_APP_API_STUB=… PLAYWRIGHT_MODULE_PATH=~/Documents/ctox/src/apps/business-os/node_modules/playwright node modules/app-store/tests/shelf.browser.mjs --output-dir <tmp>` | `SHELF_BROWSER_PASS` (erstes Öffnen 1086 ms / 8073 Box-Pixel, Wiederöffnen 938 ms, frische Canvas, offline 3 Einträge) |
| Screenshots | `tests/shots/app-store-1280.png`, `tests/shots/app-store-420.png` (im v4-Test, Shell-CSS geladen) | selbst angesehen: 1280 Navigation · Liste · Detail; 420 gestapelt, kein waagrechter Überlauf |

v4-Test deckt ab: mount zeigt den Katalog aus `business_apps`/`business_app_versions` (je genau ein Abo), Titel über
`api.shell.setTitle`, keine Zustandskästen · §9.6 „lokal angepasst“ nur als Zoneneintrag (`app-store.update.locally_modified.<id>`,
info), nicht im Inhalt · lokal angepasste App: Aktualisieren fragt über `api.shell.dialog` (ohne Ja kein Befehl) ·
Live: neue App erscheint, gelöschte verschwindet, Zone räumt sich, wenn die App auf die neue Version gehoben wird ·
Plattform-Befehl `ctox.module.update` ohne Warten (Fortschrittsknopf sofort), `sim.failCommand` → genau ein Zoneneintrag
`app-store.command.ctox.module.update` (error, `ref` = Befehls-ID), Fortschritt endet · Entfernen über Dialog, mit Ja
`ctox.app_store.uninstall` → `done` · Versionen aus `business_app_versions`, Dialog im App-root · App-Sprung und Creator über
`api.shell.navigate` · snapshot → structured clone → Remount erhält Bereich, Auswahl, Ansicht, Detail **und den
Suchbegriff sichtbar im Feld** · leere Plattform: kein Text, keine Zone · Screenshots · unmount: Abos 0, root leer,
Zone leer, keine `pageerror`.
**Nicht** abgedeckt: Zip-/GitHub-Installation im Browser (Dateidialog), Freigabe-Assistent im Browser, Hot-Plug-Skript,
Geometrie-Labor. „Schreiben (put → accepted)“ entfällt: die App schreibt keine eigenen Collections (einziger Schreibweg
ist `api.files.put` beim Zip-Upload).

## Stellen und Entscheidungen

- **R1/R2**: `mount(root, api, {restored})` → `{unmount, snapshot}`; Abos je Montage (`owner`), abgelöste Montagen räumen
  nur ihre eigenen Abos; Befehlsstatus-Abos (`state.commandOffs`) enden mit `unmount` bzw. mit der Ablösung.
- **R3 Daten**: `business_module_catalog` (`findOne('module-catalog')`, Projektion des Peers) → zwei Live-Abos auf die
  Plattform-Collections `business_apps` und `business_app_versions` (`ctox-platform`, fremd gelesen, §16.17). `rebuildCatalog()`
  leitet daraus das bisherige Katalogformat ab (Version aus `manifest.version`, Kategorie aus `manifest.layout.category`,
  Modifikation aus `manifest.provenance.locally_modified`, Basis-Hash aus `provenance.base_hash`, Versionsliste aus
  allen `business_app_versions` der App, absteigend nach `migrated_at_seq`). „Update verfügbar“ = es gibt eine Version
  mit höherem `migrated_at_seq` als die aktive (Ableitung, OFFEN-3). Verweigertes Abo → Zone `app-store.data.<collection>`.
- **R4/R12**: Readiness, `startCollection`, Katalog-Nachladeknopf („GitHub aktualisieren“, `refreshMarketplace`) und
  Sync-Zeile entfallen – Live-Abo. Zip-Upload: Chunk-Collections + Leasing (`desktop_file_chunks`, `bulkUpsert`) →
  `api.files.put(file, {id, name, mime})`; der Befehl trägt `file_id` und `file_ref` (sha256, lokal berechnet).
- **R5 Befehle**: `runStoreCommand` → `api.command(type, payload, {record_id: module_id})` ohne Warten; Karte zeigt den
  Fortschritt bis `done`/`failed`; `failed` und `h.local`-Ablehnung → `api.status.report({level:'error',
  code:'app-store.command.<type>', ref: h.id})`, `done` → `clear`. Payloads unverändert wie v3.
- **R6/R13**: Benachrichtigungen → Zone; Icons app-lokal (`actionIcon`, OFFEN-D); `openModule` und Creator-Rückweg
  (früher `sessionStorage` + Hash) → `api.shell.navigate(appId, params)`.
- **R7**: `confirm`/`prompt` → `api.shell.dialog({kind:'confirm'|'prompt', …, default:false|''})`. Versionen- und
  Freigabe-Dialog liegen im App-root (`els.root.append`), nie auf Shell-Ebene.
- **R8**: `loadModuleMessages` (HTTP) → `locales/*.json` als JSON-Import des Artefakts; `api.shell.t('app-store.<key>')`
  übersteuert; Locale live über `api.shell.locale.subscribe`.
- **R9 Rechte** (Spiegel, Entscheidung im Core): wie v3-Rückfall in `shared/permissions.js` – Owner/Chef/Admin dürfen
  installieren, aktualisieren, ändern, freigeben, entfernen; sonst muss das Recht (`apps.install`, `apps.uninstall`,
  `apps.modify`, `apps.release`) in `api.actor.permissions` stehen. App ändern zusätzlich mit `apps.source.edit`
  (PORTING R9). Nicht-öffentliche Apps sichtbar mit `apps.view`. Governance-Grants und Founder-Zuweisung je Modul gibt
  es in 4.0 nicht (OFFEN-6).
- **Fachverhalten**: Aktualisieren einer lokal angepassten App fragt wie v3 nach und sendet `mode:'discard'` (der
  Zwischenstand hatte den Knopf still wirkungslos gemacht – korrigiert). §9.6 „Update verfügbar, App lokal angepasst“
  steht nur in der Zone (für Admin/Owner/Chef bzw. `apps.source.edit`), nie in Liste oder Detail.
- **R10**: `ensureStylesheet`/`?v=` entfernt, `index.html` per `fetch(new URL('./index.html', import.meta.url))`,
  Vorschaubilder `assets/previews/<id>.png` als eigene Artefaktdateien (relativ, kein `?v=`). Regal-Import
  `../../vendor/store-shelf/store-shelf.mjs` ohne Cache-Buster (OFFEN-7).
- **Icons fremder Apps**: der Pfad `../<id>/icon.svg` (HTTP-Abruf quer über Apps) entfällt. Detail und Liste zeigen
  `manifest.layout.icon_svg` inline (bereinigt) oder die Glyphe; das Regal nutzt `api.shell.assetUrl(appId,
  layout.icon_asset || 'icon.svg')`, **nur wenn die Shell sie anbietet** (OFFEN-1), sonst Motivbox ohne Icon.
- **R11 Zustandstexte**: `data-store-message`, Lade-Overlay „GitHub Discovery wird synchronisiert“, Quellen-Info-Kasten,
  Leerzustände (`emptyCatalogTitle/Body`), Karten-Endmeldungen (`operationMessageHtml` → leer) und der Sync-Tooltip
  (`marketplaceStateLabel`) entfallen samt CSS (`.store-loading-*`, `.store-message`, `.store-empty-state`,
  `.store-sidebar-extras`, `.store-info-box`, `@keyframes app-store-sync-pulse`) und Locale-Schlüsseln.
- **R14**: Imports aus `ctox:shared/` (`app-lifecycle.js`, `permissions.js`, `roles.js`, `shell-permissions-ui.js`,
  `pane-grammar.js`); `autoWirePaneGrammar(root)` im `mount`.
- **R16**: `scope-css.mjs --write`; `.app-store-module > .ctox-modal` bleibt im Scope.
- **R17**: `schema.js` gelöscht. `collections.schema.json` gleichwertig (beide Collections tiefengleich, Migrationen
  `business_commands` 1/2 als Operationsliste). Beide Collections sind Plattform-Collections; die App besitzt keine
  eigene Collection (Manifest-Strang, OFFEN-8). Der Zwischenstand hatte `schema.js` gelöscht, solange
  `modules/mail/schema.js` es noch importierte; erst nach dem mail-Port (42f399538) ist das Löschen folgenlos (geprüft:
  kein Importeur mehr).
- **Snapshot**: `selectedId, scope, query, viewMode, drawerOpen, centerBand, categoryFilter, sortKey`, Scroll der
  Liste. Neu: nach dem Remount stehen Suchbegriff, Band und Filter wieder in den Bedienelementen
  (`restoreGrammarControls`) – vorher filterte die Liste nach einem unsichtbaren Suchbegriff.
- **Tests**: `app-store.test.mjs` lädt `index.js` über `tests/ctox-shared-loader.mjs` (Node-Loader bildet die
  SW-Umschreibung `ctox:shared/` nach); Quelltext-Wächter auf den v4-Vertrag umgestellt (kein Nachladeknopf, kein
  Cache-Buster, Zip über `api.files`, Overlays in `els.root`). `app-store-mount.test.mjs` und
  `app-store-shelf-lifecycle.test.mjs` prüfen dieselben Regeln am neuen `mount(root, api)`. `tests/shelf.browser.mjs`
  läuft gegen den gepinnten Stub.

## Befehlstypen – Einstufung (§16.13)

Rust-Bezüge `grep -rn '"<type>"' src/core --include='*.rs' | wc -l` im Klon. Alle **PLATTFORM** (Kern, App-Lebenszyklus
§9.2–§9.6 bzw. Queue), Einstufung laut `V4-HANDLERS.md`.

| Befehlstyp | Rust | UI-Sender | Payload (unverändert v3) |
|---|---:|---|---|
| ctox.app_store.install | 30 | Marketplace-Installieren, GitHub-Repo, Zip, GitHub-Update | `module_id, source_kind, repo, git_ref, subpath, download_url, source_path, manifest_url` bzw. `file_id, file_ref` (Zip, neu: Datei über `api.files`) |
| ctox.app_store.uninstall | 5 | Deinstallieren (Dialog) | `module_id` |
| ctox.module.update | 4 | Aktualisieren | `module_id, mode ('vanilla'|'discard'), expected_baseline_sha256` |
| ctox.module.check_updates | 3 | Nach Updates suchen (GitHub-Quelle) | `module_id` |
| ctox.module.rollback_version | 9 | Versionen-Dialog | `module_id, version_id` |
| ctox.module.release | 22 | Freigabe-Assistent | Assistenten-Payload |
| ctox.business_os.app.modify / business_os.chat.task | 26 / 154 | kein UI-Sender (nur Test-Haken `appStoreContextChatDetail`; v3-Kontextmenü war toter Code) | – |

## OFFEN (nummeriert)

1. **`api.shell.assetUrl` fehlt im Vertrag** (weder `app-api.d.ts` noch Stub 9fa9948 noch Architektur §16.15/§16.19).
   Gewünscht: `api.shell.assetUrl(appId, path) → string` (versionierte Artefakt-URL `/~a/<app>@<ver>~shell@<ver>/<path>`
   der aktiven Version einer anderen App, für `layout.icon_asset`). Die App ruft sie feature-geprüft; ohne sie zeigt das
   Regal keine fremden Icons (Liste/Detail nutzen `layout.icon_svg`).
2. **Marketplace/Templates ohne Datenquelle**: v3 las nicht installierte Katalog-Apps und Templates aus
   `business_module_catalog.marketplace/templates`. In 4.0 gibt es nur `business_apps` (installiert, §9.6). Bereich
   „Marketplace“ zeigt deshalb 0, „Templates“ nur „Neue App per KI-Prompt“. Der Standardbereich bleibt wie in v3
   „Marketplace“ (leer). Kern muss eine Release-Katalog-Collection (z. B. `business_release_catalog`) anbieten.
3. **Plattform-Befehlsvertrag**: Payload von `ctox.module.update/rollback_version/release` in 4.0 nicht festgelegt; die
   App sendet die v3-Felder. Vergleich-und-Setzen nach §9.2 Nr. 2 (`base_app_ver`, Zielversion) fehlt im Vertrag.
   „Update verfügbar“ ist eine Ableitung (höheres `migrated_at_seq`); ein Core-Feld wäre eindeutiger.
4. **Folgekarte (§16.19 Nr. 9, treu portiert)**: `itemHasUpdate` vergleicht die Anzeige-Texte „Installiert: x“ und
   „Katalog: lokal“; dadurch steht jede installierte App ohne Marketplace-Gegenstück im Band „Updates“ (v3 identisch).
5. **Mehrere Fenster**: Modulzustand ist modulweit; eine zweite Montage löst die erste ab (wie customers OFFEN-5).
6. **Rechte ohne Modul-Scope**: v3-Governance (explizite Grants je Modul, Founder-Zuweisung) hat in 4.0 kein Gegenstück;
   `api.actor.permissions` ist eine flache Liste. Ein Founder mit `apps.source.edit` darf im Spiegel jede App ändern
   (Core entscheidet). Wunsch: Rechte mit Scope oder `api.actor.can(permission, scope)`.
7. **Vendor-Import** `../../vendor/store-shelf/store-shelf.mjs` liegt außerhalb des App-Artefakts; unter der SW-URL
   `/~a/app-store@…/` löst der relative Pfad nicht auf. Gilt ebenso für calendar, coding-agents, documents, notes,
   spreadsheets. Braucht `ctox:vendor/…` oder Aufnahme ins Artefakt.
8. **Manifest** (`v4/manifests/app-store.manifest.json`, nicht in diesem Strang): führt `schema.js` in `files`;
   `foreign_collections` enthält Fehltreffer `documents`, `notes`, `spreadsheets` (nur Vorschau-IDs) – tatsächlich gelesen
   werden nur `business_apps`, `business_app_versions`; `files` braucht zusätzlich Schreiben (`api.files.put`).
9. Kein git benutzt.

## ASTORE2 – Kern §16.29 (15.10.2026)

Erledigt damit: OFFEN-1 (assetUrl), OFFEN-2 (Datenquelle Marketplace/Vorlagen), OFFEN-3 (Befehlsvertrag), OFFEN-4
(Updates-Band). Stub-Pin 9fa9948 kennt **weder `app_catalog` noch `ctox.app.*`** (geprüft: kein Treffer in
`~/.local/state/shell-v4/stub-pins/9fa9948`). Die Kernantwort ist deshalb **nur im Test** simuliert:
`tests/catalog-core-sim.mjs` (app_catalog als Live-Abfrage; Vergleich-und-Setzen auf `base_app_ver`; Ablehnung als
write_result `{status:'rejected', reason, detail}` ohne Eintrag in `business_commands`; angenommene Befehle laufen
über den echten Stub und ändern `business_apps`/`business_app_versions` per `sim.push`). Die App kennt nur `api.*`.

- **Daten**: vier Live-Abos `business_apps`, `business_app_versions`, `app_catalog`, `app_drafts`. Marketplace = je App
  der neueste `app_catalog`-Eintrag (Version absteigend, bei Gleichstand main vor instance); Vorlagen = Einträge mit
  `layout.template === true` oder Kategorie „Template(s)/Vorlage(n)“ (Annahme, Kern legt das Merkmal fest). Leerer
  Katalog = leere Liste ohne Text. Quelle im Detail: „CTOX main“ / „Diese Instanz“.
- **Update verfügbar** nur, wenn `app_catalog` eine höhere Version mit `source:'main'` als die aktive führt
  (`compareVersions` auf `app_ver`); `instance`-Einträge sind kein Update. Band „Updates“ zeigt nur nicht lokal
  angepasste Apps. Angepasste App mit Update: Abzeichen „Update <ver> · angepasst“ am Eintrag (`data-update-customized`)
  plus der bisherige §9.6-Zoneneintrag; Aktualisieren nur über den Verwerfen-Dialog.
- **Befehle** (`runAppCommand`): Payload `{app_id, base_app_ver, …}`, Basis = `business_apps.active_ver` wie der Client
  sie sieht. update `{release_ver}` · rollback `{target_ver}` (aus dem Versionen-Dialog) · release `{channel}` (Dialog auf
  Kanal + Datenbereiche reduziert) · remove · publish `{draft_id}` (Knopf „Veröffentlichen“, wenn ein eigener Entwurf in
  `app_drafts` liegt). Installieren aus dem Katalog = `ctox.app.update` mit `base_app_ver: null` (Annahme analog §16.22
  A3). GitHub-/Zip-Installation bleibt `ctox.app_store.install` (externe Quelle), Update-Prüfung GitHub bleibt
  `ctox.module.check_updates`.
- **Ablehnungen**: `stale_base`/`conflict` → Zone `app-store.conflict.<app>` (warn) mit Text „<Aktion> abgelehnt: Basis X
  ist nicht mehr aktiv (aktiv: Y)“ und Aktion „Auf aktuelle Version setzen“ (Eintrag weg, App mit aktiver Version im
  Detail; der nächste Befehl nimmt sie als Basis; kein automatisches Wiederholen, wie `source-mode` rebase). Andere
  Ablehnungen (z. B. Rollback auf älteres Schema ohne Rück-Migration) → `app-store.command.<typ>` (error) mit dem
  `detail` des Kerns, ohne Aktion. Eine Ablehnung erzeugt genau einen Eintrag (der parallele Status `failed` schweigt).
- **assetUrl**: `v4/shell/app-host.js` liefert `api.shell.assetUrl(appId, path)` generisch aus `kernel.apps`
  (`activated_ver`) als `/~a/<app>@<ver>~shell@<shell_ver>/<pfad>`; unbekannte App/keine aktive Version/leerer Pfad
  → `null`. Shell-Version aus der eigenen Artefakt-URL (`/~a/shell@<ver>/`), sonst `dev`, übersteuerbar über
  `api.shellVer`. Rahmen-Labor-Test ergänzt (Form gegen `sw.js` URL_RE geprüft). Das Regal lädt fremde Icons darüber.

OFFEN (ASTORE2):
- A1. `discard_local: true` in `ctox.app.update` nach bestätigtem Verwerfen-Dialog ist ein Zusatzfeld außerhalb des
  Vertrags; der Kern muss es annehmen oder Update angepasster Apps ganz ablehnen.
- A2. Template-Merkmal in `app_catalog.layout` und „Installieren = update mit base null“ sind Annahmen (s. o.).
- A3. Begründungscodes der Ablehnung (`stale_base`, `migration_required`) sind aus Stub/Simulation übernommen; echte
  Kerncodes beim Kern-Import gegenprüfen. Lesbarkeit von `app_drafts` für app-store im echten Kern ungeprüft.
- A4. Lab-Server (`v4/frame/lab/serve.mjs`) liefert `~shell@`-URLs nicht aus; geprüft wird nur die URL-Form.

## VENDOR (Strang VENDOR, 15.10.2026)
- Punkt 7 erledigt: `store-shelf/` und seine Abhängigkeit `three/` liegen in `modules/app-store/vendor/`; Import `./vendor/store-shelf/store-shelf.mjs`.
