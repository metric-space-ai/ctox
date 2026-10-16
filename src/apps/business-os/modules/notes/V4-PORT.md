# notes – Port auf App-API 4.0 (Strang UB3, Welle B)

Stand: abgeschlossen 15.10.2026 13:37 (Berlin, gemessen). Start 13:20.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app notes` | vorher 41 Stellen + 215 CSS-Selektoren ungescoped → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs notes modules/notes/index.css --check` | gescoped |
| Löschlisten-Wächter | `node v4/tools/guards/guard-v4.mjs --json` (Filter `modules/notes/`, auch `--include-tests`) | vorher 20 → **0** Treffer |
| v4-Test gegen Stub-Pin | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test modules/notes/tests/v4-api.test.mjs` | **9/9 grün** (Chromium headless 1223) |
| Bestehende Suite | `node --test modules/notes/notes.test.mjs` | **13/13 grün** (auf v4-Vertrag umgestellt, esbuild aus `~/Documents/ctox/src/apps/business-os/node_modules`) |
| Screenshots | `NOTES_SHOTS=1 …v4-api.test.mjs` | `tests/shots/notes-1280.png`, `tests/shots/notes-420.png` angesehen: Liste + Editor (1280), gestapelter Editor mit Zurück (420), keine Zustandstexte |

v4-Test deckt ab: mount zeigt Daten, Titel über `api.shell.setTitle`, ein Abo · `sim.push` erscheint live, Löschung
verschwindet · leere Collection mit Rückstand: kein Text, keine Zone, kein Seeding; ohne Rückstand Beispielnotizen wie
bisher · leere Suche rendert nichts und die Liste kommt zurück · Entwurf speichern → `api.put`, live, Hinweis nur in der
Zone · `api.put` direkt → `accepted` · Papierkorb fragt über `api.shell.dialog` (ohne Ja nichts), mit Ja `patch`,
Rückgängig-Knopf im Fenster, Zone geräumt · snapshot → structuredClone → Remount erhält Entwurf, Auswahl · unmount
löst alle Abos und leert root · keine `pageerror`.

## Was sich geändert hat

- **Einstieg** `mount(root, api, {restored})` → `{unmount, snapshot}`; Zustand je mount zurückgesetzt, Abos je Montage.
- **Daten**: ein Live-Abo `api.query('notes', {sort:[{updated_at_ms:'desc'}]})` statt `collection.find().exec()` +
  `collection.$` + Neuladen aller Dokumente bei jeder Änderung. Lokale Entwürfe bleiben bei neuen Emissionen erhalten.
  Schreiben über `api.put/patch/remove` (`writeNotes`), Existenzprüfung über `api.get(...).get()` statt `findOne`.
  `local`-Ablehnung → `notes.write.invalid` in die Zone; Core-Ablehnungen meldet der Kernel.
- **Readiness raus**: `collectionReadiness`/`subscribeCollectionReadiness` und der Leerzustand „Notizen werden
  synchronisiert.“ entfallen. Leere Liste rendert nichts (auch keine Such-/Leertexte; `buildNotesEmptyState` gelöscht).
- **Seeding** (Fachverhalten): leere Collection wird einmal mit 3 Beispielnotizen gefüllt – jetzt nur, wenn
  `api.metrics.get().backlog === 0` (sonst ist eine leere erste Emission kein Beweis für eine leere Collection; vorher
  ohne jede Prüfung). Entscheidung dieses Strangs.
- **Zustandstexte raus**: Sync-Anzeige „Synchronisiert/Speichern…/Gespeichert/Save failed“ (index.html + JS),
  Locale-Schlüssel `saving, saved, statusReady, localSync, syncDescription, syncHint, syncingNotes, presenceEditing`.
  Hinweise (Entwurf gespeichert/verworfen, Import, Favorit, Papierkorb …) gehen als `info` mit Code `notes.notice` in
  die Zone und werden nach 6,5 s geräumt. Der Rückgängig-Knopf bleibt als reiner Knopf im Fenster (`.nn-action-toast`).
  Verschlüsselungsfehler → `notes.encrypt.failed`.
- **Dialoge**: `confirm/prompt/alert` → `api.shell.dialog({kind:'confirm'|'prompt', title, text, default})`
  (destruktiv `default:false`, Passwort mit `secret:true`).
- **Shell**: `setTitle`, Sprache über `api.shell.locale` (live) + `t('notes.<key>')` mit eingebetteten Rückfalltexten
  (Schlüssel aus `locales/*.json` in `labels` übernommen; `shared/i18n.js` entfällt). `autoWirePaneGrammar(root)` ruft die
  App selbst; wiederhergestellte Suche/Band/Sortierung werden ins Grammatik-DOM gespiegelt.
- **Presence** (`ctx.presence.set/subscribe`, Badge „bearbeitet gerade“) entfällt: keine 4.0-API (OFFEN-2).
- **Imports** `ctox:shared/sanitize-rich-html.js`, `ctox:shared/stable-dom.js`, `ctox:shared/pane-grammar.js`.
- **Markup/CSS**: `fetch(new URL('./index.html', import.meta.url))` ohne `?v=`; Stylesheet-Injektion entfernt; CSS
  unter `[data-app="notes"]` (19 Shell-Vorfahren ersetzt). Die v2-Freihaltungen für Icon-Zone (80+14 px links) und
  Fensterknopf (78 px rechts) entfernt: in v4 liegt das Chrome außerhalb von root (Entwurf §6).
- **Fehlerbehebung**: leere Liste setzte `innerHTML` an `renderHtmlIfChanged` vorbei → nach einer Suche ohne Treffer
  blieb die Liste leer (Signatur unverändert). Jetzt `renderHtmlIfChanged(list, '', {signature:'empty'})`; im v4-Test
  belegt.
- **Snapshot**: `activeCategory, activeNotebook, activeTag, activeNoteId, searchQuery, sortMode, viewMode,
  editorCollapsed`, ungespeicherter Entwurf (`draft`), ungespeicherter Inhalt der offenen Notiz (`contentDraft`, nur
  unverschlüsselt), `listScrollTop`. Entschlüsselte Inhalte und Passwörter verlassen die Montage nie.
- **schema.js gelöscht**: gleich zu `collections.schema.json` (Version 1, `required` id/title/updated_at_ms,
  Migration 1 = 6 Operationen). **Abweichung**: `conflictStrategy: 'field-merge'` stand nur in schema.js und fehlt in
  collections.schema.json → OFFEN-1.

## Befehlstypen – Einstufung (§16.13)

notes sendet **keine** Befehle (kein `commandBus`, kein `api.command`). Rust-Bezüge `grep -rn '"notes"' src/core`
wurden nicht gezählt (keine Befehlstypen). Keine Handler nötig.

## OFFEN

1. `conflictStrategy: 'field-merge'` (gleichzeitige Änderungen an Titel/Metadaten vs. Inhalt überleben beide) ist im
   v4-Manifest nicht ausdrückbar bzw. nicht übernommen → Manifest-Strang / „ctox sync v4“ (Feld-Merge im Core).
2. Presence (wer bearbeitet dieselbe Notiz) hat in App-API 4.0 keine Entsprechung → Wunsch an den Vertrag.
3. `import * as Lexical from '../../vendor/lexical.mjs'` greift aus der App heraus; unter `/~a/notes@<ver>/` löst das
   auf `/vendor/lexical.mjs` auf. Ob der SW `vendor/` ausliefert oder ein `ctox:vendor/`-Schema kommt, ist offen.
4. Die App-CSS hängt an der Container-Query `business-app-window` (1024/768) – die v4-Shell muss den Container um
   root setzen (Entwurf §6 „Haltepunkte bleiben Shell-Eigentum“). Der Test setzt ihn auf `#win`.
5. Modulzustand ist modulweit (wie knowledge OFFEN-4): zwei gleichzeitige Montagen teilen `state`.
6. Rückgängig-Knopf als Fensterelement ohne Text: Owner-Abnahme, ob das als „Zustandsanzeige“ gilt.

## VENDOR (Strang VENDOR, 15.10.2026)
- Punkt 3 erledigt: `lexical.mjs` und `lexical-src/` liegen in `modules/notes/vendor/`; Import `./vendor/lexical.mjs`; Build: `src/scripts/vendor-builds/build-business-os-vendor.mjs --only=lexical`.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G1: Formatleiste und Notiz-Metazeile brechen um statt (mit versteckter Leiste) seitlich zu scrollen.
- G4: Kartentitel brechen um; Textvorschau (`.notes-card-snippet`) bleibt einzeilig mit `data-v4-allow-ellipsis` (Vorschau, fachlich gewollt).
- Offen (Rahmen): Kopf-Icons rutschen unter den Titel, weil `.ctox-pane-title-row` (flex) nur in app.css steht.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app notes --shared` (Stub-Pin 9fa9948) → alle Läufe OK; `node --test modules/notes/tests/v4-api.test.mjs` grün; `scan.mjs --app notes` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.
