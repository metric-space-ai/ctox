# coding-agents – Port auf App-API 4.0

## Runde AGENT (15.10.2026, abends) – neue Rolle: Übersicht der Agent-Läufe

Die App ist **keine Werkbank mehr**. Der Agent sitzt als Seitenleiste in der Code-Ansicht jeder App
(`v4/source/agent-panel.js`, Bericht `v4/source/AGENT.md`). `index.html`/`index.js`/`index.css` sind neu: eine Spalte
mit Standard-Pane-Kopf, Suche und den Läufen aus `coding_turns`; Klick → `api.shell.openSource(app_id)`. Entfernt:
Projekt-Leiste mit eigenem Kopf-Icon und Abstandshalter (M-04), Chat/Transkript, Delegieren-Formular, Modellwahl,
Ergebniskarte, Export, Sitzungs-Banner, `autoWirePaneGrammar`, `vendor/chat-ui`-Nutzung, `api.command` aus der UI.

| Prüfung | Ergebnis |
|---|---|
| `node --test modules/coding-agents/tests/*.test.mjs` (Pins 68b83a3, 74137b1, 51b1e33) | 8/8 |
| `node --test modules/coding-agents/handlers/tests/*.mjs` | 4/4 (Handler unverändert) |
| AG-7 im Rahmen-Labor (Pin 51b1e33) | Läufe gelistet, Titel rechts neben dem Shell-Icon, Klick öffnet Code-Ansicht mit Seitenleiste |
| `node v4/tools/guards/guard-v4.mjs` | 0 Treffer unter `modules/coding-agents/` |

Damit überholt: der Abschnitt unten (Strang UC3) beschreibt die alte Werkbank; OFFEN 2 (modulweiter Zustand) und
OFFEN 5 (Alt-Test) sind erledigt (Zustand je Montage; `tests/coding-agents.test.mjs` prüft die neue Darstellung).
OFFEN 1 neu: `v4/manifests/coding-agents.manifest.json` muss neu erzeugt werden (Deklaration `coding_turns: read` ist
eingetragen; Strang MAN2).

---

## Verlauf: Strang UC3 (alte Werkbank, überholt)

Status: **UI portiert, Abnahme grün** (Berlin-Zeit 2026-10-15).

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app coding-agents` | 0 Stellen, Exit 0 (vorher 22 Stellen, CSS 159 ungescoped) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs coding-agents modules/coding-agents/index.css --check` | gescoped |
| Zustandsvokabular | `node v4/tools/guards/guard-v4.mjs --json` (modules/coding-agents/) | 0 Treffer (verbleibender Treffer liegt in v4/manifests/, Fremd-Strang) |
| v4-Test (Kern-Stub-Pin cff5ba5) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/cff5ba5 node --test modules/coding-agents/tests/v4-api.test.mjs` | 7/7 grün, 0 pageerror |
| Screenshots | `tests/shots/coding-agents-{1280,420}.png` | gemountet, Live-Apps + Workbench sichtbar, kein horizontaler Überlauf |

v4-Test deckt ab: mount zeigt Live-Apps (business_apps) · `sim.push` erscheint, Löschung verschwindet · leere
Collections rendern keinen Lade-/Sync-Zustandstext und keine Zone · Delegieren legt `business_commands`-Dokument an,
`sim.failCommand` → genau ein Zoneneintrag `coding-agents.command.ctox.coding.turn` · `sim.onCommand` done → Ergebnis
(applied_files) landet in der Ergebniskarte, Erfolg bleibt still · `snapshot()`/`restored` erhält Auswahl und offenen
Entwurf, alte Abos abgemeldet · `unmount` leert root und meldet alle Abos ab.

## Was sich geändert hat

- **Einstieg** war bereits auf `mount(root, api, {restored}) → {unmount, snapshot}` umgestellt; die Helfer darunter
  liefen noch auf dem alten `ctx`-Vertrag und wurden nachgezogen.
- **Daten**: `loadApps`/`loadActiveSession`/`loadSessionEvents`/`loadRecentTurns` lesen jetzt die vom mount-Abo in
  `state.data[<coll>]` gepushten Live-Zeilen (business_apps, business_commands, coding_agent_sessions,
  coding_agent_events) statt `ctx.modules`/`ctx.getModules()` bzw. `ctx.db.collection(...).find().exec()`.
  Die Query liefert sortiert und ohne gelöschte Dokumente; die alten `readCollectionDocs`/`readableCollection`/
  `toPlainDoc`/`filterReadableCollectionNames`/`newId` sind entfernt.
- **Die Warte-Stellen sind weg**: `loadModelCapabilities` (`until:'terminal'`, Timeout) und `delegateTurn`/
  `dispatchCodingTurn` (`until:'accepted'`, `await`) rufen jetzt `api.command(type, {...payload, record_id, module,
  client_context})` und warten **nie**. Beide abonnieren `handle.status`: `failed` → `api.status.report`
  (Code `coding-agents.command.<type>`), `done` → `clear` + (Modelle: Preset-Liste, Turn: Ergebniskarte) aus dem
  gepushten `CommandState.result`. `handle.local` meldet lokale Ablehnung in die Zone. Fortschritt des Turns kommt
  live aus `business_commands` (Delegationen-Liste), nicht aus einem Warte-Ergebnis.
- **Sync/Readiness/Polling weg**: `collectionReadiness`, `transcriptSourcesNotReady`, `renderSyncingShell`,
  `subscribeReadinessUpdates`, `subscribeProjectionUpdates`, `scheduleProjectionRefresh`, `clearProjectionTimers`
  ersatzlos entfernt. Leere Transcript-/Delegationen-Liste rendert die normale Leermeldung, keinen Sync-Zustand.
- **Zustandstexte weg**: `.ctox-syncing` + `syncingData` entfernt; Lade-Platzhalter im `index.html` (`Apps werden
  geladen…`) und Loading-Branch in `renderAppList` entfernt (die Live-Query liefert sofort den aktuellen Stand).
  Validierungsmeldungen (`emptyTask`/`taskTooShort`) gehen über `api.status.report` (Code
  `coding-agents.task.invalid`) statt in den Formular-Hint. Locales um `loadingApps`/`syncingData` bereinigt,
  Standalone-Note-Vokabular entschärft.
- **Shell/Icons/Nav**: Titel über `api.shell.setTitle`; Header-Icons app-lokal inline (4.0 hat keine Icon-API,
  OFFEN-D); `ctx.openDesktopApp('code-editor', …)` → `api.shell.openSource(mod.id)` (Quelltext-Modus der Shell; code-editor ist in v4 keine App, FR4-5);
  Deep-Link in die App über `api.shell.onRoute`.
- **Actor**: `actorContext(ctx.session)` in den Befehls-Client-Contexts entfernt – die Entscheidung trifft der Core;
  die UI projiziert keinen Actor mehr ins Befehlsdokument.
- **Snapshot**: offener Aufgaben-Entwurf (`taskDraft`) wird im `snapshot()` mitgenommen und beim Remount in
  `updateFormState` zurückgespielt, sobald die ausgewählte App live und das Eingabefeld aktiv ist (vorher ging der
  Entwurf verloren – `pendingTaskDraft` wurde gesetzt, aber nie angewandt).
- **HTTP/Download**: `downloadJsonFile` nutzt `els.root` statt `ctx.host` als Anker.
- **schema.js gelöscht**: 3 Collections (coding_agent_workspace_grants/sessions/events), Version 0, leere
  Migrationen, additionalProperties true – inhaltlich gleich zu `collections.schema.json` (gleiche `required`,
  `version`, `indexes`). Keine Abweichung.

## Befehlstypen – Einstufung (§16.13)

Die 2 Befehlstypen sind im Handler-Strang H8 bereits portiert (siehe `V4-HANDLERS.md`, `handlers/`):
`ctox.coding.models` und `ctox.coding.turn`, beide **FÄHIGKEIT** (`ctx.cap.agent.coding.{models,turn}`,
pi-Sidecar unter `src/core/coding_agents/`, Rust `store.rs:15883-15998`). Die UI ruft beide ausschließlich über
`api.command` (nie wartend). `handlers/**` wurde NICHT angefasst (Eigentum H8).

## OFFEN (nummeriert)

1. **Manifest** (`module.json`, Strang v4/manifests): `handlers`, `capabilities_required`
   (`ctx.cap.agent.coding.*`), `foreign_collections` (`business_apps`, `business_commands`), `styles` (index.css
   lädt die Shell) fehlen bzw. müssen aus der Generierung kommen. Nicht angefasst (nicht im Whitelist).
2. **Mehrere Fenster**: Modulzustand ist modulweit (ein `const state`, ein `let railChip`/`chatView`); zwei
   gleichzeitige Montagen teilen `state`. Leck ist behoben (Abos je Montage, im v4-Test nachgewiesen), echte
   Mehrfenster-Fähigkeit braucht Zustand je Instanz (nicht in diesem Strang).
3. **Shell-Lücken 4.0**: keine Icon-API (OFFEN-D), `t()`-Schlüsselraum offen (OFFEN-B) – coding-agents nutzt das
   eingebettete `labels`-Objekt mit `api.shell.t`-Übersteuerung.
4. **Screenshot-Treue**: der bare Stub-Harness liefert keine Shell-Design-Tokens/Pane-Grid; die Screenshots belegen
   Mount + Live-Apps + Workbench + kein Überlauf, aber nicht die finale Optik (Shell-Strang).
5. **Alt-Test** `tests/coding-agents.test.mjs` prüft den alten `ctx`-Vertrag und die jetzt entfernten Hooks
   `turnErrorFromProjection`/`filterReadableCollectionNames`; er ist NICHT auf v4 umgestellt (Folgearbeit) und
   nicht gelöscht. Der neue `tests/v4-api.test.mjs` deckt den v4-Kontrakt ab.
