# kundenpipeline (Decision Hub) – UI-Port auf App-API 4.0 (Strang UB3, Welle B)

Stand: abgeschlossen 15.10.2026 14:20 (Berlin, gemessen). Start 14:17. Handler (`handlers/**`, `V4-HANDLERS.md`)
gehören dem Handler-Strang und sind unberührt.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app kundenpipeline` | vorher 13 Stellen (+ CSS ungescoped) → **0 Stellen**, Exit 0 |
| CSS-Scope | `node v4/tools/port/scope-css.mjs kundenpipeline modules/kundenpipeline/index.css --check` | gescoped |
| Löschlisten-Wächter | `guard-v4.mjs --json` (Filter `modules/kundenpipeline/`, ohne `handlers/`) | vorher 9 → **1** (`assets/icon/provenance.json`, nicht in meiner Dateiliste, OFFEN-1) |
| v4-Test gegen Stub-Pin | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/66edb98 node --test modules/kundenpipeline/tests/v4-api.test.mjs` | **8/8 grün** |
| Bestehende Suite | `node --test modules/kundenpipeline/tests/kundenpipeline.test.mjs` | **7/7 grün** (auf v4-Vertrag und collections.schema.json umgestellt) |
| Screenshots | `KUNDENPIPELINE_SHOTS=1 …` | `tests/shots/kundenpipeline-1280.png`, `-420.png` angesehen; Layoutfehler der Liste behoben (s. u.) |

v4-Test deckt ab: mount (3 Abos, Titel „Decision Hub“, Liste + Detail), **kein `setInterval`**, kein
Aktualisieren-Knopf · `sim.push` live, Löschung verschwindet · leere Collections: Liste, Detail, Aktionen, Kicker, Titel
leer, keine Zone · **Befehl** `kundenpipeline.decision.resolve` (Agent-Option) über `api.command` ohne Warten,
`sim.failCommand` → genau ein Zoneneintrag `kundenpipeline.command.kundenpipeline.decision.resolve`, keine
Statuszeile im Inhalt; zweiter Lauf `done` räumt; `kundenpipeline.decision.answer` mit `vorgang_id`/`wert` · `api.put`
→ `accepted`, live · snapshot → structuredClone → Remount erhält Suche, Filter, Auswahl, Aktionsspalte · unmount löst
alle Abos, leert root · keine `pageerror`.

## Was sich geändert hat

- **10-s-Neuladeschleife ersatzlos entfernt** (`setInterval(refresh, 10000)` + `find().exec()` aller drei
  Collections): drei Live-Abos `api.query(<coll>, {})`. Der Aktualisieren-Knopf (`data-refresh`) entfällt mit ihr.
- **Readiness raus**: `subscribeCollectionReadiness`, `collectionReadiness`, `isLoadingEmpty` und „Entscheidungen werden
  synchronisiert …“.
- **Befehle**: `commandBus.dispatch` (mit Warten auf `outcome`) → `api.command(command_type, {...payload, record_id,
  client_context})`, nie warten. Status live in die Zone: `failed` → error (mit Grund), `done` → räumen. Die
  Statuszeile im Aktionsbereich („Wird an CTOX übergeben …“, „Entscheidung gespeichert.“, „Aktion fehlgeschlagen …“,
  „CTOX Command Bus ist nicht verfügbar.“) entfällt. `decisionCommand()` (Fachkern, Test-Hook) unverändert.
- **Zustandstexte raus**: Leerzustände („Keine passenden Entscheidungen.“, „Wähle eine … aus.“, „Für diese Entscheidung
  sind keine Aktionen mehr verfügbar.“, „Keine Frage hinterlegt.“, „Kein Kontext“, Platzhalter „Keine Auswahl /
  Entscheidung auswählen“, „Wird geladen …“, „Der Command wird serverseitig policy-geprüft.“, Ladefehler im Fuß).
  Locale-Schlüssel `syncing`, `noDecisions`, `noSelection` entfernt. Der Fuß zeigt „x von y Entscheidungen“ (wie die
  Spaltenfüße der anderen Apps).
- **Shell**: `setTitle('Decision Hub')`; Sprache für Datumsformat über `api.shell.locale`.
- **Markup/CSS**: `ensureStyles` entfernt (Shell lädt CSS); CSS unter `[data-app="kundenpipeline"]`.
  **Layoutfehler behoben**: die Rail hat 4 Zeilen, das Kit-Raster `.ctox-pane` nur 2 → die Ansichtsleiste bekam den
  freien Platz und die Liste rutschte ans untere Ende (im 1280-Screenshot gesehen); jetzt
  `grid-template-rows: auto auto minmax(0,1fr) auto`.
- **Snapshot**: Auswahl, Filter, Suche, Sichtbarkeit der Aktionsspalte, Listen-Scroll.
- **schema.js gelöscht** (importierte `../ctox/schema.js?v=…`): 4 Collections, Versionen, `required`, Felder, Indizes
  identisch mit collections.schema.json (gemessen); business_commands-Migration 1/2 steht dort deklarativ
  (`set_from_first_truthy inbound_channel/module`). Der Test prüft jetzt die JSON-Fassung.

## Befehlstypen – Einstufung (§16.13)

Einstufung und Handler: `V4-HANDLERS.md` (Handler-Strang). UI sendet zwei Typen; Rust-Treffer
`grep -rn '"<type>"' src/core --include=*.rs` (gemessen 14:18):

| Befehlstyp | Rust-Treffer | Einstufung (V4-HANDLERS.md) |
|---|---:|---|
| `kundenpipeline.decision.resolve` | 3 | Fachlogik, portiert |
| `kundenpipeline.decision.answer` | 3 | Fachlogik, portiert |

## OFFEN

1. Wächter-Treffer `vocab:en-unavailable` in `assets/icon/provenance.json` (Herkunftsnotiz des Icons, „image
   generation was unavailable“) – kein UI-Text, Datei nicht in meiner Dateiliste → Wächter-Ausnahme für `assets/` oder
   Asset-Strang.
2. `module.json` (nicht meine Datei) trägt noch `layout.shell_contract: "v2"`, `business_commands` in `collections`
   und keine `handlers` → Manifest-Generator.
3. Modulzustand ist modulweit (zweite Montage übernimmt, wie knowledge OFFEN-4).
4. Unbenutzte CSS-Regel `.kundenpipeline-command-status` bleibt stehen (harmlos; Aufräumen mit dem Löschlisten-Wächter).
