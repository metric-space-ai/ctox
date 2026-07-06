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
| `sync` | facade | `startCollection`, `stopCollection`, `restartCollection(s)`, `suspendCollections`, `resumeCollections`, `stop`; Getter `mode`, `config`, `diagnostics` (inkl. per-Collection `pullCheckpointAgeMs`, §OS-A3). |
| `syncConfig` | object | Momentaufnahme der Sync-Konfiguration. |
| `storageScope` | facade | Modul-gescopte Ablage-Schlüssel (localStorage-Scoping). |
| `runtimeCapabilities` | frozen object | Trust-/Fähigkeitsmatrix (`business-os-runtime-capabilities-v1`): trust_model, code_origin, database.guarded, … |

### Aktionen & Arbeit

| Feld | Typ | Bedeutung |
|---|---|---|
| `commandBus` | facade | `dispatch(command)` → `business_commands` (server-geprüft, Capability-Token wird angeheftet). Der einzige Mutationsweg für serverwirksame Aktionen. |
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
| `getSvgIcon(name)` | fn | Registrierte Shell-Icons. |
| `isTaskbarPinned(target)` / `pinToTaskbar` / `unpinFromTaskbar` / `toggleTaskbarPin` | fn | Taskbar-Pinning. |

### Fehler & Meldungen

| Feld | Typ | Bedeutung |
|---|---|---|
| `reportFileIntegrityError(error, details)` | fn | Datei-Integritätsfehler an die Shell melden (modul-attributiert). |
| `reportIssue(details)` | fn | Problem am aktuellen Modul melden. |

## Stabilität

- v1 umfasst exakt die 41 oben gelisteten Felder (Pin im Assert-Skript).
- Additiv erweitern ist erlaubt (Pin + Doku im selben Commit).
- Entfernen/Umbenennen ⇒ `business-os-module-context-v2` mit
  Migrationshinweisen für alle Module.
- Module dürfen nur dokumentierte Felder nutzen; undokumentierte
  Shell-Interna (z. B. `state`-Zugriffe) sind kein Vertragsbestandteil.
