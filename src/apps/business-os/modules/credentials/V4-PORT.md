# credentials – Port auf App-API 4.0 (Strang UB1, Welle B)

Stand 15.10.2026 14:20 (Berlin, gemessen). Vorlage: `v4/tools/port/PORTING.md`, Pilot knowledge. `handlers/` und
`V4-HANDLERS.md` gehören dem Handler-Strang H8 und sind unverändert.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app credentials` | **0 Stellen**, Exit 0 (vorher 32, CSS 51 ungescoped, 14 mit Shell-Vorfahr) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs credentials modules/credentials/index.css --check` | gescoped |
| v4-Test (Stub-Pin 66edb98) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test src/apps/business-os/modules/credentials/tests/v4-api.test.mjs` | 8/8 grün |
| Bestehende Suite | `node --test modules/credentials/credentials.test.mjs` (Scratch-Kopie mit node_modules-Link, esbuild + `ctox:shared/`-Plugin) | 17/17 grün |
| guard-v4 (ohne handlers/) | gefiltert | vorher 28, jetzt **0** |
| Screenshots | `tests/shots/credentials-1280.png`, `credentials-420.png` | angesehen: 1280 Liste + Detail + Formular; 420 gestapelt, kein Überlauf |

v4-Test: Liste kommt als `done`-Ergebnis von `ctox.secret.list` (genau ein Listenbefehl beim Öffnen) · put/delete eines
anderen Geräts (`sim.push` in `business_commands`) löst genau eine Neuliste aus, Eintrag erscheint/verschwindet · leer:
kein Text, keine Zone · Wert setzen → `ctox.secret.put` `done`, Wert **nicht** im lokalen Befehlsdokument, Liste
aktualisiert, Erfolg in der Zone; `sim.failCommand` → genau ein Zoneneintrag `credentials.command.ctox.secret.put` ·
Löschen fragt über `api.shell.dialog` (default false → kein Befehl) · snapshot/restored: Auswahl, Ansicht, eingegebener
Schlüssel – der Wert nie im Snapshot · unmount 0 Abos, root leer.

## Was sich geändert hat

- **Geheimnis-Metadaten über `ctox.secret.list`**: wie gefordert als `api.command(type, payload)` mit gepushtem Status
  umgesetzt; die Liste ist `status.result` (`{catalog, extra}` → `mergeEntries`), nie erwartet (`await`), Single-Flight
  bleibt. Fehler → `credentials.list.failed` / `credentials.command.<type>`.
- **Reaktivität**: `api.query('business_commands', {selector: put/delete auf type ODER command_type})` statt
  `find().$` – Signaturvergleich (Feldbericht 11.09.2026) unverändert; erste Emission löst keine Liste aus (die macht
  der mount selbst).
- **put/delete**: `api.command`, Erfolg → Zone (`credentials.secret.saved|deleted`, früher `ctx.notifications`),
  Löschen bestätigt `api.shell.dialog({kind:'confirm', default:false})` statt `window.confirm`.
- **Lokale Schwärzung** (`redactLocalCommand`): nach Terminalzustand `api.patch('business_commands', id,
  {payload:{value:null, name}})` (JSON-Merge-Patch, null entfernt) statt RxDB-`doc.patch` – siehe OFFEN-2.
- **Rechte-Spiegel**: `api.actor.permissions` enthält `secrets.manage` oder Rolle admin/chef (früher
  `canUseBusinessPermission(session, governance)`); live über `permissions.subscribe`. Ohne Recht: Liste leer,
  Hinweis `credentials.permission.missing` in der Zone statt Kasten im Inhalt.
- **Zustandstexte** raus: Sync-Kasten, `empty_all`, `empty_filtered`, `syncing`, `empty_known`, `refresh` (JS +
  locales); Formular-/Importhinweise → Zone (`credentials.form.key_invalid|value_required`,
  `credentials.import.invalid|empty|done`).
- `buildCommandDoc` liefert jetzt `{type, payload}` für `api.command` (Routing-Felder im Payload); Test angepasst.
- `ctox:shared/pane-grammar.js` (`autoWirePaneGrammar(host)`), `shared/i18n.js` und `shared/permissions.js?v=…`
  entfallen; `ensureStyles`/`?v=` entfernt; CSS gescoped, App-root Größencontainer.
- **schema.js gelöscht**: deckt nur die System-Collection `business_commands` (Version 2) ab; inhaltlich gleich zu
  `collections.schema.json` inkl. `migration_strategies` 1 (`set_from_first_truthy inbound_channel/module`) und 2 (leer).

## Befehlstypen (Einstufung siehe `V4-HANDLERS.md`, Strang H8)

| Befehlstyp | Rust (`"<type>"`, .rs) | Einstufung |
|---|---:|---|
| `ctox.secret.list` | 7 | Plattform/Fähigkeit → `handlers/ctox.secret.list.js` → `cap.secret.list` (H8) |
| `ctox.secret.put` | 9 | Plattform/Fähigkeit (+ Schlüsselprüfung) → Handler (H8) |
| `ctox.secret.delete` | 4 | Plattform/Fähigkeit (+ Schlüsselprüfung) → Handler (H8) |

## OFFEN

1. **Wunsch an „ctox sync v4“: Geheimnis-Metadaten als Collection über `ctx.cap.secrets`.** Heute holt die App die
   Metadaten per Befehl `ctox.secret.list` und hört auf put/delete-Befehle in `business_commands`, um neu zu listen
   (Befehls-Roundtrip als Lesepfad). Ziel: der Core/Handler projiziert die Metadaten (`{name, description, is_set,
   source, updated_at}`, **nie** Werte) über `ctx.cap.secrets` in eine Collection (z. B. `business_secret_meta`), die
   App liest sie mit `api.query(...).subscribe` wie jede andere Collection; `ctox.secret.list` und das
   business_commands-Abo entfallen dann.
2. **Schwärzung des Werts im Befehlsdokument** gehört in den Core (Annahme des Befehls, §8): die App schreibt in die
   System-Collection `business_commands` (`api.patch` mit `value:null`), weil der Stub (wie der lokale Outbox-Eintrag)
   den Klartext bis dahin hält. Ob Apps `business_commands` patchen dürfen, ist im Vertrag offen.
3. `business_commands` ist die einzige Collection der App (System-Collection, R17): Manifest-Strang klärt, ob sie aus
   `collections` fällt.
4. **Bild**: Write-only-Hinweis (statischer Erklärtext im Formular, kein Zustand) bleibt; der linke Kopf reserviert noch
   die Shell-V2-Iconzone (Titel gekürzt) – Shell-Strang.
