# desktop – Port auf App-API 4.0 (Strang UC4, Welle C)

Status: **fertig** (14:14 Berlin).

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app desktop` | vorher 115 Stellen (CSS 69 ungescoped) → **0**, Exit 0 |
| Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/desktop` | vorher 54 → **0** |
| CSS-Scope | `node v4/tools/port/scope-css.mjs desktop modules/desktop/index.css --check` | gescoped |
| v4-Test (Stub-Pin 67229d2) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/67229d2 node --test src/apps/business-os/modules/desktop/tests/v4-api.test.mjs` | **6/6 grün** |
| Bestehende Suite (umgestellt) | `node src/apps/business-os/modules/desktop/tests/desktop.test.mjs` | grün (app-commands, desktop-menu-actions neu auf api, responsive, icon-drag) |
| Screenshots | `tests/shots/desktop-1280.png`, `tests/shots/desktop-420.png` | angesehen: Icons im Raster, Auswahlrahmen, Kompaktraster bei 420; Uhr-Widget per bestehender Container-Regel (≤ 1320 px) ausgeblendet; kein Überlauf |

v4-Test deckt ab: mount zeigt Icons live aus `desktop_icons`, legt Standard-Icons je `business_apps`-Eintrag
(`desk_icon_<id>`) und das Layout-Dokument an, kein Sync-/Leer-/Statustext, keine Zone, Titel, Uhr läuft · `sim.push`
ändert Beschriftung live, `hidden` lässt das Icon verschwinden · Enter → `api.shell.navigate(target)` · Kontextmenü
über `api.shell.menu` → „Icon entfernen“ = `api.patch(hidden)`, „Frage stellen“ → `navigate('ctox', {chat})` ·
„App umbenennen“ → `api.command('ctox.module.save')` ohne Warten, `sim.failCommand` → genau ein Zoneneintrag, Icon-
Beschriftung lokal sofort · snapshot/restored (Auswahl, Rücksetz-Sicherung) · unmount: 0 Abos, root leer.

## Was bleibt App, was macht der Rahmen

**Bleibt in desktop** (Arbeitsflächen-Hintergrund): Desktop-Icons (Anzeige, Ziehen/Umordnen, Raster aus
`desktop_layout`), Kontextmenüs Fläche/Icon (App Creator/Explorer/Notizen öffnen, ausrichten, nach Namen sortieren,
fehlende Standard-Icons, Wartung: zurücksetzen / vorherige Anordnung), Icon umbenennen/entfernen, App umbenennen
(`ctox.module.save`), Kontext-Chat-Einstiege, Uhr/Datum-Widget.

**Aus desktop entfernt – der Rahmen (v4/frame) muss übernehmen** (nichts in v4/frame geändert):
1. **Anheften an die Taskleiste**: Menüpunkte „An Bar anheften/Von Bar lösen“, Ablegen eines Taskleisten-Pins auf der
   Fläche (`application/x-ctox-taskbar-pin` → lösen), Icon in die Topbar ziehen (`onDragToTopbar` ist jetzt leer).
   Vorher über `ctx.isTaskbarPinned/toggleTaskbarPin/pinToTaskbar/unpinFromTaskbar`. Pins liegen in
   `desktop_layout.taskbar_pins` (desktop legt den Standard beim ersten Start weiter an).
2. **Launcher-Katalog**: vorher `ctx.modules`/`ctx.getModules`/`registry.json` + `ctx.eventBus 'modules:changed'`,
   `ctx.getDesktopApps`. desktop liest jetzt nur `business_apps` live (Plattform); Startmenü/Katalog ist Rahmen.
3. **Live-Aktivität „CTOX live“**: Toast je neuem `business_commands`-Dokument fremder Module (vorher
   `ctx.notifications.show` mit Öffnen-Knopf) → Zustandszone/Rahmen.
4. **Sync-Anzeige** (`widget-sync`: „Sync läuft · Wartet auf …“, Zähler, Balken, 2-s-Takt,
   `ctox-business-os-sync-diagnostics`) und Puls „CTOX Plattform aktiv“ → Zustandszone (`api.metrics` liest der Rahmen).
5. **Benachrichtigungen** über Erfolg („Icons ausgerichtet/sortiert/hinzugefügt“, „App umbenannt“) entfallen; Fehler
   gehen als `desktop.action`, `desktop.write.<…>`, `desktop.command.ctox.module.save` in die Zone.
6. **Kontext-Chat-Oberfläche**: vorher `ctx.openBusinessChat` bzw. Fensterereignis `ctox-business-os-chat-open`. Jetzt
   `api.shell.navigate('ctox', {chat: detail})` – die CTOX-App (oder der Rahmen) muss `params.chat` annehmen.
7. **Icon-Glyphen der Shell** (`ctx.getSvgIcon`): jetzt nur `ctox:shared/icons.js` (OFFEN-D, keine Icon-API).

## Was sich geändert hat

- `mount(root, api, {restored})` → `{unmount, snapshot}`; `snapshot()` = `{v:1, selectedIconId, resetBackup?}`.
- Daten: drei Live-Abos (`business_apps`, `desktop_layout/layout` über `api.get`, `desktop_icons`), Schreiben über
  `api.put/patch/remove`. Entfallen: `layout-authority.js` (native Layout-Autorität `readNativeCollectionDocument`,
  `withDesktopIconReconciliationRead`, Konfliktbehandlung `insert`), Readiness-Abos, Positions-Cache und Zeilen-
  Migrationsflag in `localStorage` (lokale Writes sind sofort sichtbar), `bulkUpsert`.
- Standard-Icons und Kollisionsausgleich einmal je Montage nach der ersten Emission (vorher je Laden + Katalogereignis).
- Dialoge: `api.shell.dialog({kind:'prompt'|'confirm', …, default})` statt `showBusinessPrompt/Confirm`.
- Kontextmenüs: `api.shell.menu({x, y, items})` → `id`; deaktivierte Einträge tragen `disabled` (vorher zusätzlich
  `disabledReason`/`onDisabled`-Meldung).
- i18n: `api.shell.locale` + `t('desktop.<key>')`, eingebettete Texte; `shared/i18n.js` (HTTP-Locales) entfällt;
  Locales ohne die 24 Zustands-/Meldungsschlüssel.
- Uhr: `setTimeout`-Kette auf die nächste Sekunde (statt `setInterval`).
- Imports `ctox:shared/icons.js`, `ctox:shared/workjet-theme.js`; keine `?v=`; `ensureStyles` entfernt.
- `ctoxLauncher.js`: `navigate` statt `location.hash`/`openApp`; `desktopMenuActions.js`: reine Funktionen über `docs`
  + `api` (`desktopIconWriteAvailability`, `dispatchDesktopChatOpen` entfallen); `iconDrag.js`: Option `onActivate` →
  `onOpen` (Scanner-Regel R1).
- Gelöscht: `schema.js`, `layout-authority.js`, `layout-authority.test.mjs`, `tests/layout-restart.test.mjs`,
  `registry-launch-smoke.mjs` (prüfte registry.json/system-apps.json/app.js und die native Layout-Autorität – alles
  Löschliste v4). `tests/desktop.test.mjs` lädt die verbliebenen vier Suiten; `desktop-menu-actions.test.mjs` neu
  gegen eine api-Attrappe; `responsive.test.mjs` auf v4-Schreibweg umgestellt.

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust-Treffer (`grep -rn '"<type>"' src/core`, .rs) | Einstufung |
|---|---:|---|
| `ctox.module.save` (App umbenennen) | gemessen im Manifest-Strang: `command_plane.rs`/`store.rs` | **Core-generisch** (App-Manifest speichern, app-unabhängig) |
| `business_os.chat.task`, `ctox.business_os.app.modify` (Kontext-Chat, über die CTOX-App) | – | Core-generisch, nicht von desktop gesendet |

## OFFEN

1. **Verweise auf gelöschte Dateien außerhalb dieser App** (nicht geändert): `app.js:968` importiert
   `./modules/desktop/schema.js`; `modules/support/schema.js:4` und `modules/invoices/schema.js:3` importieren
   `../desktop/schema.js`; `scripts/assert-rxdb-only.mjs:685` liest `modules/desktop/schema.js`.
2. **desktop_files / desktop_file_chunks / channel_pairing_state** stehen im generierten Manifest unter desktop, werden
   aber von desktop nicht gelesen: `desktop_files` nutzt explorer (schreibend, explorer/V4-PORT.md OFFEN-1),
   Chunks entfallen (§10.2a Nr. 4). Eigentum klären (Manifest-Strang).
3. **Rücksetz-Sicherung** lebte in `localStorage` (überlebte Neuladen); jetzt nur Montage + `snapshot`. Dauerhaft
   bräuchte es ein Feld/Dokument in `desktop_layout` (Schema hat keins).
4. **Chat-Übergabe** `navigate('ctox', {chat})` ist ein Vorschlag; die CTOX-App muss den Parameter lesen.
5. Tote CSS-Regeln für `.widget-sync`, `.widget-status`, `.status-pulse`, `.desktop-icon-empty` stehen noch in
   index.css (gescoped, ohne Treffer im Markup).
6. `responsive.test.mjs` und `icon-drag.test.mjs` sind Quelltext-/Geometrie-Wächter ohne Browser; das Geometrie-Labor
   (3 Breiten) läuft nicht in diesem Strang.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G6: Kompaktraster folgte dem Viewport (`@media (max-width: 767px)`); jetzt `@container (max-width: 767px)` auf `.desktop-module`, die Rastervariablen sitzen auf `.desktop-icons` (der Container kann sich nicht selbst abfragen). `tests/responsive.test.mjs` prüft das und verbietet Breiten-`@media`.
- „Desktop bleibt trotz Daten leer“: kein App-Mangel. Im Labor ist nur `desktop` selbst in `business_apps`, die Fixture-Icons zeigen auf `standard`/`email` (keine Apps); Verknüpfungen auf unbekannte Apps blendet desktop absichtlich aus. Der v4-api-Test (Katalog mit explorer/notes) zeigt die Icons.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app desktop --shared` (Stub-Pin 9fa9948) → G6 grün 3/3; `node --test modules/desktop/tests/v4-api.test.mjs` grün; `scan.mjs --app desktop` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.
