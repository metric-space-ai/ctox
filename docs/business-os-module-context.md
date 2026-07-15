# Business OS Module Context (`mount(ctx)`) — Contract v1

Stand: 2026-07-06
Contract-ID: `business-os-module-context-v1`

Jedes Business-OS-Modul — statisch oder runtime-installiert, von Hand oder
per Agent gebaut — erhält beim Start genau EIN Objekt: `mount(ctx)`. Dieses
Dokument ist die verbindliche Feldliste. Sie wird mechanisch gepinnt durch
`src/apps/business-os/scripts/assert-module-context-contract.mjs` gegen die
`CTX-CONTRACT-BEGIN/END`-Marker in `src/apps/business-os/app.js`
(`createModuleContext`). Ein neues Feld erfordert ein Update von Pin UND
diesem Dokument im selben Change; ein entferntes/umbenanntes Feld ist ein
Breaking Change der Modul-API und braucht eine explizite Entscheidung plus
Versionssprung.

Grundregeln (aus `docs/ctox-rxdb.md` und den AGENTS-Guardrails):

- Module importieren NIE `rxdb` oder bauen eigene Sync-/HTTP-Datenwege —
  alles läuft über die hier gereichten Facades.
- Browser-Facades spiegeln Rechte für die UX; autoritativ bleibt die
  Server-Policy (`policy.rs`). Eine UI-Sperre ersetzt keinen Server-Check.
- Presence ist advisorisch und darf nie eine Aktion gaten.

## Felder

### Identität & Umgebung

| Feld | Typ | Bedeutung |
|---|---|---|
| `module` | object | Der Katalogeintrag dieses Moduls (`module.json`-Projektion: id, entry, …). |
| `modules` | array | Aktueller Modulkatalog (Momentaufnahme). |
| `getModules()` | fn → array | Live-Zugriff auf den Katalog. |
| `locale` | `'de'`\|`'en'` | UI-Sprache. |
| `shellStyle` | `'macos'`\|`'windows'` | Shell-Optik-Variante. |
| `session` | object | Server-gelieferte Session (`user`, Auth-Zustand). |
| `actor` | object | Normalisierte Identität: `id`, `email`, `login`, `display_name`, `role`, `is_admin`. |
| `user` | object | `actor` gemergt mit dem Session-User (Anzeige-Identität). |
| `args` | object | Launch-Argumente aus Shell-/Fensterstart, inklusive Hash-Query-Parametern für windowed Module. |

### DOM-Anker & Fenster

| Feld | Typ | Bedeutung |
|---|---|---|
| `host` | Element | Wurzel-Container des Moduls (`[data-module-content]`). |
| `left` / `right` | Element | Linke/rechte Shell-Slots (leeren, wenn ungenutzt). |
| `windowManager` | object | Shell-Fensterverwaltung (Fokus, Fenster-Lifecycle). |
| `openLeftDrawer(content)` / `openRightDrawer(content)` / `openBottomDrawer(content)` | fn | Shell-Drawer öffnen. |
| `closeDrawers()` | fn | Alle Drawer schließen. |

### Daten & Sync (der einzige Datenweg)

| Feld | Typ | Bedeutung |
|---|---|---|
| `db` | facade | Guarded/scoped Collection-Zugriff (Live-Facade; runtime-installierte Module bekommen den Data-Guard). Kein direkter Bundle-Import. |
| `documents` | facade | Generischer DOCX-Vertrag: `loadVersion`, `createDocx`, `open`. Bytes laufen ausschließlich über `documents`, `document_versions` und `document_blob_chunks` im shell-gelieferten `db`-Facade. |
| `sync` | facade | `startCollection`, `stopCollection`, `restartCollection(s)`, `suspendCollections`, `resumeCollections`, `stop`; Getter `mode`, `config`, `diagnostics` (inkl. per-Collection `pullCheckpointAgeMs`, §OS-A3). |
| `syncConfig` | object | Momentaufnahme der Sync-Konfiguration. |
| `storageScope` | facade | Modul-, Workspace- und Actor-gescopte UI-Ablage: `key`, `get`, `set`, `remove`. Module greifen nicht direkt auf Browser Storage zu. |
| `runtimeCapabilities` | frozen object | Trust-/Fähigkeitsmatrix (`business-os-runtime-capabilities-v1`): trust_model, code_origin, database.guarded, … |

### Aktionen & Arbeit

| Feld | Typ | Bedeutung |
|---|---|---|
| `commandBus` | facade | `dispatch(command, { until })` sowie `submit`, `waitForAccepted`, `waitForTerminal`, `resumeTracking`, `subscribe`, `getStatus`, `cancel`. Der Bus erzeugt den kanonischen v2-`business_commands`-Datensatz, heftet das native Capability-Token an und verfolgt den Lifecycle. Apps schreiben niemals direkt in die Collection. `command_type` ist autoritativ; `type` bleibt nur ein gleichwertiger Eingabealias und abweichende Doppelangaben werden abgewiesen. |
| `actions` | facade | Benannte App-Aktionen über `run(name, input, options)`. Serverseitige oder Collection-übergreifende Aktionen delegieren über den Command Bus; deklarative lokale Aktionen bleiben auf `read`, `assert`, `insert`, `upsert`, `patch`, `delete` und `emit` begrenzt. |
| `contextActions` | facade | Context v2: `register(element, descriptor)` registriert explizite Surface-/Pane-/Entity-/Field-/Selection-Ziele und liefert eine Cleanup-Funktion; `capture(target, pointer?)` erfasst den normalisierten Kontext; `dispatch('ask'|'data'|'app', options)` sendet immer über den Typed Command Bus. |
| `businessChat` | facade | `open(detail)`, `submitTask(options)` — CTOX-Arbeit aus der App auslösen. |
| `openBusinessChat(detail)` | fn | Chat-Panel öffnen (Kontextübergabe). |
| `canModifyModule()` | fn → bool | UX-Spiegel der Modify-Policy (Server bleibt autoritativ). |
| `permissions` | facade | UX-Spiegel der Rechte (`canWriteCollection`, …). |

### Mehrbenutzer-UX

| Feld | Typ | Bedeutung |
|---|---|---|
| `presence` | facade | `set(entries)`, `clear()`, `subscribe(listener)` — advisorische "wer schaut/bearbeitet was"-Hinweise (ctox-presence-v1). Actor wird von der Shell gestempelt; niemals autorisierend. |
| `notifications` | object | Shell-Benachrichtigungen. |
| `eventBus` | object | Shell-interner Event-Bus (Modul↔Shell-Signale). |
| `contextMenu` | object | Shell-Kontextmenü (Right-click→Agent-Kontext läuft hierüber). |
| `governance` | object | Governance-Zustand der Shell. |

### Desktop-Integration

| Feld | Typ | Bedeutung |
|---|---|---|
| `desktopApps` | array | Momentaufnahme der Desktop-Apps. |
| `getDesktopApps()` | fn → array | Live-Liste. |
| `openDesktopApp(id, options)` | fn | Desktop-App öffnen. |
| `getSvgIcon(name)` | fn | Registrierte Shell-Icons (Modul-/Kachel-Icons, Gradient-Stil). |
| `getActionIcon(name, size?, strokeWidth?)` | fn → string | Funktionale Aktions-Icons (monochrom, `currentColor`) für `.ctox-pane-icon`/`.ctox-icon-button`; Namen via `listActionIcons()` in `shared/icons.js`. |
| `isTaskbarPinned(target)` / `pinToTaskbar` / `unpinFromTaskbar` / `toggleTaskbarPin` | fn | Taskbar-Pinning. |

### Fehler & Meldungen

| Feld | Typ | Bedeutung |
|---|---|---|
| `reportFileIntegrityError(error, details)` | fn | Datei-Integritätsfehler an die Shell melden (modul-attributiert). |
| `reportIssue(details)` | fn | Problem am aktuellen Modul melden. |

## Documents-Facade

`ctx.documents` ist der generische App-zu-Documents-Vertrag. Er stellt genau
diese Methoden bereit:

- `loadVersion({ documentId, versionId?, expectedSha256 })` lädt Dokument,
  Version und alle nach `idx` geordneten Base64-Chunks, prüft Vollständigkeit
  und SHA-256 und liefert `{ document, version, bytes, sha256, filename,
  mimeType }`. `expectedSha256` ist verpflichtend.
- `createDocx({ filename, mimeType, bytes, idempotencyKey, title?, ownerId?,
  linkedRecords?, templateRef?, provenance? })` akzeptiert ausschließlich den
  MIME-Typ
  `application/vnd.openxmlformats-officedocument.wordprocessingml.document`,
  einen sicheren `.docx`-Dateinamen und nichtleere Binärdaten. Die Facade
  schreibt Base64-Chunks mit 256000 Zeichen pro Chunk, danach Version und
  Dokument. `linkedRecords`, `templateRef` und `provenance` werden als
  `linked_records`, `template_ref` und `provenance` auf Dokument und Version
  erhalten. Derselbe `idempotencyKey` liefert bei identischem Inhalt den
  bestehenden Datensatz; Konflikte oder Teilzustände werden abgewiesen.
- `open({ documentId, versionId? })` ruft den generischen App-Launcher als
  `openDesktopApp('documents', { args: { record: documentId, version? } })`
  auf. Die Facade manipuliert weder DOM noch URL selbst.

Alle drei Collections werden über den zum aufrufenden Modul gehörenden,
guardierten `ctx.db`-Facade aufgelöst. Fehlende Collections, ungültige
Chunk-Metadaten, Hashabweichungen und unvollständige idempotente Datensätze
schlagen geschlossen fehl. Es gibt keinen HTTP- oder Storage-Fallback.

## Stabilität

- v1 umfasst exakt die 45 oben gelisteten Felder (Pin im Assert-Skript;
  `getActionIcon`, `args` und `documents` kamen additiv hinzu).
- Additiv erweitern ist erlaubt (Pin + Doku im selben Commit).
- Entfernen/Umbenennen ⇒ `business-os-module-context-v2` mit
  Migrationshinweisen für alle Module.
- Module dürfen nur dokumentierte Felder nutzen; undokumentierte
  Shell-Interna (z. B. `state`-Zugriffe) sind kein Vertragsbestandteil.

## Context Actions v2

Der kanonische Kontext verwendet `schema_version: "business-os-context-v2"`
und enthält `app_id`, `window_instance_id`, `surface_id`, `pane_id`,
`presentation_mode`, `entity.collection/type/id/label`, `field.path`,
`selection.ids/text`, `pointer.x/y` und `deep_link`. Die bisherigen flachen
v1-Felder sowie `surface`, `location`, `client_x/client_y` bleiben während der
Migration als Dual-Read-Aliase erhalten. ContextMenu-Taste und Shift+F10 öffnen
denselben zentralen Ablauf wie ein Rechtsklick.
