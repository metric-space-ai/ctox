# Business OS Roles and Permissions Plan

Related Threads remediation: `docs/business-os-threads-roles-remediation-plan-2026-06-25.md`

Status: Locally production-ready for the Phase 0-12 implemented core
roles/permissions surfaces, dynamic-app lifecycle gates and native lifecycle
catalog projection. Phase 10 is complete locally: App Store release publishes a
private runtime-installed `0.x` app to Team `1.0.0`, proves data-review,
projection, reload, storage-boundary and rollback, and Settings Activity shows
redacted release/rollback audit evidence through the real Browser/Rust path.
Phase 11 is complete locally for audience visibility, deep-link locking,
responsibility safety and lifecycle badge/drawer/launcher UX evidence. Phase
12 is complete locally for MCP app visibility/data split, visible agent scopes,
client-context integrity, read-only grant-boundary UX and native/MCP audit
metadata parity.
Phase 13E/13F are complete locally for the dynamic-app runtime-safety and
browser-storage boundaries: runtime-installed/generated apps expose an explicit
same-origin trusted-code capability contract, the installed-app validator plus
Browser/Rust dynamic app smoke prove the covered network/import/storage/
Shell-global/cached-DB/external-effect bypasses are blocked, and UI preference
storage is now scoped by workspace/actor where relevant while remaining
non-authoritative for app visibility, release state, audience state and data
grants.
Phase 13C has a packaged/starter user-module migration batch locally verified:
`coding-agents`, `calendar`, `buchhaltung`, `conversations`, `customers`, `cv-print-builder`,
`documents`, `invoices`, `iot`, `matching`, `notes`, `outbound`, `research`, `shiftflow`, `spreadsheets` and
`support` now use the guarded DB facade. The Dynamic Apps Browser/Rust smoke proves
collection/property/raw/context deny-before-grant, `ctx.permissions` parity,
a real Support Shell locked state without data grants, read-after-grant and
write-without-`data.write` denial; `coding-agents`, `calendar`, `customers`,
`buchhaltung`, `invoices`, `iot`, `matching`, `notes`, `outbound`, `research` and `shiftflow` are now smoked against their module-owned
`coding_agent_sessions`, `calendar_events`, `accounting_journal_entries`, `customer_accounts`,
`accounting_invoices`, `iot_widgets`, `matching_requirements`, `notes`, `outbound_campaigns`, `research_tasks` and `planning_shifts`
collections.
Phase 13 system raw cleanup has also removed direct `ctx.db.raw` access from
Browser, CTOX, Knowledge, Reports and Tickets; those modules now resolve their
system collections through `ctx.db.collection(name)`. Creator's runtime and
generated-app template also no longer use collection-property or
`ctx.db.collections` fallback access. Phase 13G has removed the remaining
guarded-module collection-property/proxy fallback paths from Buchhaltung,
Calendar, Coding Agents, Customers, CV Print Builder, Documents, Invoices, IoT,
Notes, Outbound, Shiftflow, Spreadsheets and Support; all 24 inventory module
entries now have raw/property/proxy/cached-handle flags set to false. Inventory
and module-conformance guards are green, Dynamic Apps Browser/Rust smoke proves
all 16 packaged guard modules with browser warnings/errors/404/request failures
0, and the Browser/Rust UI-regression smoke opens and interacts with CTOX zoom,
Browser refresh, Knowledge tabs, Tickets filters, Creator and Reports filters
through the real Shell. Phase 13H has closed the previously unscoped
Settings, Desktop-app, Business Chat and Business Reporter facades with
explicit collection allowlists; the DB-isolation inventory now reports 0
unscoped facades. Phase 13 scoped system/internal exceptions are now closed
too: App Store, Browser, Creator, CTOX, Desktop, Knowledge, Reports and Tickets
are served by `SCOPED_SYSTEM_MODULE_DB_COLLECTIONS`, the inventory stores exact
`scoped_collections`, and Dynamic Apps Browser/Rust evidence proves 8/8 scopes
with foreign collection denial. Phase 13 is complete locally. The latest
UI-regression smoke is functionally green but not warning-clean because Chrome
emits the known contenteditable-in-flex advisory from the Notes editor path.
Phase 14A smoke-mode registry is implemented, and Release, Audience, Agent
Scope, Auth Scope, Fresh Profile and Restore/Resync are now implemented
production smokes for their covered browser stories; Dynamic Apps browser stories are also
implemented for their covered slices. Auth Scope proves the real browser login,
authenticated reload, logout, logged-out reload, protected-access block,
tenant-scope stability and browser-storage tampering path. Fresh Profile proves
clean browser profile startup, authoritative projection, lifecycle/version
labels, disabled reasons, desktop/narrow viewports and no storage widening.
Restore/Resync proves a clean browser profile can keep a local IndexedDB write
while the native peer is stopped and then converge it back to native SQLite via
WebRTC after peer restart without HTTP fallback or browser errors.
Native backup/restore manifests now snapshot the CTOX Secret Store, carry a
Secret-Store-backed HMAC-SHA256 signature, declare same-version/downgrade
compatibility policy and carry local raw-backup retention/support-attachment
rules. The CLI drill now also creates a chunked AES-256-GCM portable snapshot
export, verifies it by decrypting/opening the ZIP, deletes temporary plaintext
ZIPs and records the encryption-key escrow requirement in the manifest/runbook;
`ctox business-os backup prune-drills` deletes only expired drill directories
with manifest retention metadata. Key escrow remains an operator process, not a
filesystem mutation performed by the drill.
The static required-smoke-mode guard is wired into CI, and Phase 16C/16D now
wire the warning-clean production Browser/Rust gate plus fixed smoke artifact
upload into CI and the tag-release workflow. Legacy migration fixtures are now
source-backed and tested, and customer/operator docs have a release dry-run
artifact. Phase 16 remains open for actual security/privacy signoff and final
customer/operator release review before any full production-ready claim.
Phase 15A "Warum?" diagnostics now has locally verified Shell, native and
Settings Browser/Rust evidence. The native `ctox.business_os.why` command
explains app visibility/open/edit/source/release/rollback and per-data-area
read/write decisions from the native lifecycle projection and central policy
engine, while persisting a sanitized command projection for this diagnostics
command. Settings module management
dispatches this command from a business-facing `Warum?` action and the live
`business-os-roles-permissions-ui` Browser/Rust smoke proves the rendered
diagnostics rows are visible and redacted without raw policy keys, prompt,
token or selection leakage. The same pass also hardens the Desktop icon
renderer so reload-time IndexedDB closing during smoke no longer produces a
browser console error.
Full product production readiness is not claimed until the remaining Phase
15/16 gates are implemented and verified. Phase 14 is closed for the current
local-workspace product claim: auth/reload, local tenant-scope stability,
fresh-profile UI, visual labels and scale budgets are Browser/Rust-proven.
Hosted/multi-workspace isolation is not claimed by this local release and
stays future hosted-product scope rather than hidden Phase-14 evidence.

Purpose: CTOX Business OS bekommt ein verstaendliches, produktnahes Rollen-
und Rechte-Modell fuer Multi-User-Setups. Das Modell soll erklaeren, wer Apps
nutzen, verwalten, per Rechtsklick aendern, veroeffentlichen, zurueckrollen,
installieren, entfernen, Agentenzugriff erlauben und kritische externe Aktionen
freigeben darf.

Dieses Dokument ist ein laufender Implementierungsplan. Agents sollen den
Phase Tracker, die Evidence-Links und den Update Log nach jeder abgeschlossenen
Arbeitsrunde aktualisieren.

## Source Validation Statement

Dieser Plan wurde am 2026-06-16 durch read-only Subagent-Source-Reviews gegen
die echte Codebasis validiert. Die Subagents haben keine Dateien editiert,
keine Tests ausgefuehrt, keine Generatoren gestartet und keine Commits erzeugt.
Am 2026-06-17 wurden Phase 5D Service-Actor-Mapping sowie die danach offenen
Record-/Approval-Ownership- und External-Effect-Punkte erneut read-only gegen
den aktuellen Source validiert; die Ergebnisse sind in Phase 5 und den offenen
Entscheidungen eingearbeitet. Phase 5E setzt davon den kleinsten hart
validierbaren Slice um: exakte Record- und Approval-Grants fuer MCP, ohne ein
neues Ownership-Ableitungssystem einzufuehren. Phase 6A setzt den ersten
Rollout-/Audit-Slice um: native Policy-Denials schreiben einen querybaren
`business_events`-Eintrag, ohne neue RxDB Collection oder Browser-Bundle-
Aenderung. Phase 6B erweitert denselben Pfad auf native Team-/Rollen- und
App-Verantwortlichkeits-Aenderungen. Phase 6C setzt den admin-sichtbaren
Activity-Slice ueber einen bestehenden `business_commands` Control-Command um,
ohne neue RxDB Collection und ohne HTTP-Datenpfad. Phase 6D erweitert diesen
nativen Activity-Pfad auf Outbound-Freigabeentscheidungen
(`outbound.message.approve`, `outbound.message.reject`,
`outbound.message.request_changes`) und bleibt ebenfalls bei `business_events`.
Phase 6E ergaenzt die operator-orientierte Rollout- und Recovery-Guidance in
`docs/business-os-roles-permissions-rollout.md`; sie fuehrt keinen neuen
Datenpfad und keine Schemaaenderung ein. Phase 6F erweitert den bestehenden
nativen Audit-Pfad auf erlaubte Policy-Entscheidungen
(`business_os.policy.allowed`) aus vorhandenen `PolicyDecision`-Gates. Der
Activity-List-Command wird fuer erlaubte Entscheidungen bewusst nicht
selbstgeloggt, damit die Activity-Ansicht kein Eigenrauschen erzeugt.
Phase 3D/4E schliesst die Owner-Transfer-Grenze: Admins und explizite
`users.manage`-Grants duerfen Nutzer weiter verwalten, aber keine Zielrolle
`owner`/`chef` setzen; Owner-Transfer erfordert `workspace.manage` und ist in
der Settings-UI nur fuer Owner auswaehlbar.
Das Deny-UI-Pattern ist fuer Phase-4-Oberflaechen akzeptiert: Routine-
Affordances wie Shell-`App ändern` werden verborgen, waehrend relevante
App-Store-Update-/Install-/Uninstall-Aktionen disabled mit Klartextgrund
sichtbar bleiben.
Die App-Source-Sichtbarkeit ist fuer den aktuellen Rollout entschieden:
Teammitglied sieht Source standardmaessig nicht; Owner/Admin,
zugewiesene App-Verantwortliche und exakte `apps.source.view`-Grants koennen
Source oeffnen. Speichern/Aendern bleibt davon getrennt und erfordert weiter
`apps.modify`.
Die MCP-External-Effect-Grenze ist fuer MCP Channel v1 und den aktuellen
Rollout entschieden: `business_os.approve` bleibt im externen Toolpfad
standardmaessig geblockt, und `business_os.execute_action` blockiert externe
Effekte auch nach gemeinsamer Policy- und Confirmation-Pruefung. Eine
approval-gated Freischaltung externer Effekte ist eine spaetere Produktphase
mit eigenem Risikomodell und Tests.
Die automatische Record-/Approval-Ownership-Ableitung ist fuer den aktuellen
Rollout ebenfalls entschieden: Business OS bleibt bei exakten Record- und
Approval-Grants plus bestehenden Collection-/Module-/Outbound-Fallbacks.
Owner-artige Payload-Felder wie `owner_id` und Approval-Felder wie
`actor_user_id` erzeugen keine impliziten Grants, bis ein normalisiertes,
produktspezifisches Ownership-Modell mit eigenen Tests definiert ist.
Der Completion-Audit am 2026-06-17 hat alle Phasen gegen den aktuellen
Worktree und die kanonischen Business-OS/RxDB-Guardrails abgeglichen. Alle
aktuellen Phase-Tracker stehen auf `Complete`; historisches `Ready for review`
oder `TBD` in alten Update-Log-Zeilen ist kein aktueller Blocker. Die
nachgezogene RxDB-Format-Guard-Luecke wurde mechanisch in sauberen
`src/core/rxdb`-Dateien korrigiert und mit `cargo fmt --check`,
`cargo test --manifest-path src/core/rxdb/Cargo.toml` und
`node src/apps/business-os/rxdb/tests/run-all.mjs` erneut verifiziert.
Phase 7 schliesst die lokale Production-Readiness-Luecke fuer diesen Slice:
ein Live Browser/Rust Smoke prueft die echten Shell-Rechtsklick-, Appbar-,
Reload- und rollenbasierten UI-Pfade; die Smoke-Matrix erzwingt harte Evidence
fuer diese Rechte-Fakten; der Wire-Daemon wurde gebaut und die RxDB Browser-
Suite laeuft ohne die vorherigen Wire-Daemon-Skips. Diese Aussage ist eine
lokale Release-Gate-Aussage fuer den Rollen/Permissions-Slice, kein Ersatz fuer
einen spaeteren CI-/Deployment-Lauf.
Phase 8 setzt den core implementation slice um:
`docs/business-os-dynamic-apps-permissions-concept.md` verbindet App-Versionen
(`0.x` privat/Vorschau, `1.0.0+` Team, explizit eingeschraenkte Team-Apps),
Icon-/Tab-Badges, Sichtbarkeit, App-Governance und KI/Agenten-Zugriff. Der
umgesetzte Slice umfasst shared Lifecycle-Logik, Shell/App-Store-Badges,
Lifecycle-Drawer, runtime-installed Sichtbarkeitsfilter fuer Team-Releases und
private `0.x`-Apps via App-Verantwortlichkeit oder exakte App-Grants sowie eine
permission-aware Browser-DB-Fassade fuer dynamische Apps inklusive guarded
`ctx.db.raw` im getesteten Helper-/Smoke-Hook-Pfad. Phase 13B hat danach die
reale Shell-Kontextfunktion `createModuleContext(mod)` auf
`createLiveDbFacade(mod)` umgestellt und Browser/Rust-Evidence fuer
Collection-, Property-, Cached-Handle- und Raw-Denial plus Cached-Handle-Read
nach explizitem Grant ergaenzt. Die persistierte Runtime-App-Mount-Kette ueber
`openModule(mod)` nach Reload/Fresh Profile und die ehemals unscoped
Desktop-/Chat-/Report-Fassaden wurden spaeter in Phase 13B/13H geschlossen;
packaged/core module exceptions bleiben als Phase-13-Produktionsblocker, bis
system/internal Policies eng getestet sind.
Vollstaendige Release-/Datenreview-Workflows im App Store und Agent-Scope-UI
sind jetzt als Phases 10-16 mit eigenstaendig testbaren Gates ausformuliert.
Phase 9 verschiebt den App-Lifecycle aus reiner Browser-Inferenz in die
native Catalog-Projektion: `module_catalog_for_rxdb` projiziert
`lifecycle`, `visibility_state`, `audience` und `release_channel` aus
Manifest, `business_module_acl`, `business_permission_grants`,
`business_module_versions` und `business_module_releases` in
`business_module_catalog.governance.lifecycle`. Shell und App Store koennen
diese Projektion konsumieren; `current_semver=null` bleibt fuer invalid/missing
SemVer autoritativ, damit keine stale Manifest-Version eine App aus Versehen
Team-sichtbar macht. Es wurde keine neue RxDB Collection, kein HTTP-Datenpfad
und kein Prozess-Env-Featuretoggle eingefuehrt.
Phase 10A setzt einen backend-only Command-Hardening-Slice um:
Release-/Source-Rollback-Helfer pruefen intern `apps.rollback` statt
`apps.modify`, Rollback-/Release-Fehler werden als fehlgeschlagene
`business_commands` persistiert, `ctox.module.list_versions` authentifiziert
und gate't den Actor ueber `apps.source.view`, `apps.release` oder
`apps.rollback`, und Data-Review-Evidence validiert read/write Collections als
Manifest-Subsets mit explizitem `grants_implied=false`. Dieser Slice ist nicht
gleich Phase-10-Produktionsreife: Grant-Reconciliation/Locked-State,
Release-/Rollback-Konsistenz, App-Store-Wizard, Settings-Fallback,
Browser-Smokes und CI-Gates bleiben offen; der damals offene No-HTTP-Blocker
wurde in Phase 10C geschlossen.
Phase 11A hat den non-public App-Sichtbarkeitsgrant vom Edit-/Source-/Release-
Modell getrennt: `apps.view` ist jetzt im nativen und Browser-Permission-Modell
vorhanden, App-Verantwortliche erhalten ihn fuer ihre zugewiesene App, und
Preview-Projektion liest nur noch aktive `apps.view`-Grants. Ein exakter
`apps.modify`-Grant macht eine private oder eingeschraenkte App nicht mehr
sichtbar. Phase 11B/11D migriert legacy `lifecycle.preview_user_ids`
idempotent in native `apps.view`-Grants, projiziert `preview_user_ids` nur noch
aus aktiven Grants, blockt direkte `openModule`-/Deep-Link-Aufrufe vor dem
Modulimport fuer Out-of-Audience-User und belegt Preview/Restricted-Reload,
Fresh-Profile und Storage-Non-Authority im Browser/Rust
`business-os-app-audience-ui` Smoke. Phase 11C verhindert jetzt orphaned
private Runtime-Apps: `ctox.module.assign_founder` und
`ctox.business_os.user.upsert` blocken die Entfernung/Deaktivierung der letzten
aktiven App-Verantwortlichen, ausser Owner/Admin bestaetigen explizit die
Recovery-Verantwortung; deaktivierte Nutzer werden aus aktiven
Verantwortlichen-/Permission-Projektionen herausgefiltert. Phase 11D schliesst
die badge-/drawer-nahe Audience-Management-UX: Launcher/Startmenue,
Shell-Tabs, Appbar und Drawer zeigen Version plus Lifecycle, sichtbare
Team-Nutzer bekommen `Nur Ansicht`, App-Verantwortliche/Manager bekommen
`Verwalten erlaubt`, und der Browser/Rust Dynamic-Apps-Smoke prueft beide
Drawer-Zustaende ueber echte Badge-Klicks.
Phase 10B schliesst den Backend-Contract fuer Data-Review-Reconciliation:
Jede im Team-Release-Review deklarierte read/write Collection muss entweder
einen aktiven expliziten Team-Rollen-Grant (`subject_type=role`,
`subject_id=user`) fuer `data.read`/`data.write` auf der Collection oder dem
Modul haben oder im Review als locked Data Area mit
`locked_state_behavior` deklariert sein. Die Reconciliation wird im
Release-Snapshot gespeichert und erzeugt weiterhin keine Grants. UI-Rendering
der locked/granted Data Areas bleibt Phase-10-Produktarbeit.
Phase 10C schliesst den No-HTTP-Blocker fuer module source/release/rollback:
`server.rs` enthaelt keine direkten Match-Arme oder Helper mehr fuer
`/api/business-os/modules/source`, `/api/business-os/modules/release` oder
`/api/business-os/modules/rollback`. Die globale Business-OS-HTTP-Datenrouten-
Sperre bleibt aktiv, und `assert-rxdb-only.mjs` failt, sobald diese Route-
Strings oder direkte Server-Marker fuer Source-Load/Save, Release oder Rollback
wieder auftauchen. Die aktiven Source-/Release-/Rollback-Pfade bleiben
`business_commands` ueber RxDB/WebRTC und werden im nativen Peer an `store.rs`
delegiert.
Phase 10D1 schliesst den Backend-Kern der Release-/Rollback-Konsistenz:
`ctox.module.release` validiert `source_version_id` und `rollback_version_id`
gegen vorhandene `business_module_versions`, bevor `module.json` geschrieben
wird; Release-Row und manuell erzeugter Source-Version-Snapshot werden in einer
SQLite-Transaktion synchronisiert; Release und Release-Rollback stellen
`module.json` wieder her, wenn die nachgelagerte SQLite-Aktualisierung
fehlschlaegt. Damit ist der konkrete Manifest/SQLite-Divergenzfall getestet,
damals blieben Projektions-/Repair-Abschluss, Settings-/Browser-Audit-Evidence,
App Store Wizard, Settings-Fallback und Browser/Rust-Smoke offen.
Phase 10D2 schliesst den Backend-Projektions-/Repair-Abschluss fuer Release-
Konsistenz: Replay desselben `ctox.module.release` Commands ist idempotent und
erzeugt keine zweite Release-Zeile, keine zweite manuelle Source-Version und
kein doppeltes Release-Audit-Event. `ctox.module.repair_lifecycle_projection`
repariert canonical `business_records` fuer `business_module_releases`,
schreibt die entsprechende RxDB-Collection-Projektion neu und synchronisiert den
`business_module_catalog` aus der echten Runtime-installed App-Root-Topologie.
Der Command ist ueber `runtime.manage` gegated und der RxDB Peer behandelt ihn
als Catalog-Sync-Command. Phase 15 bleibt fuer breitere Operator-Recovery-
Drills, Support-Artefakte und Runbooks zustaendig.
Phase 10E2 schliesst den nativen Backend-Projektions-Slice fuer Release-State:
`business_module_catalog.governance.lifecycle` enthaelt nun `release_state`,
`release_status`, `rollback_target` und eine business-faehige
`data_access`-Zusammenfassung fuer granted/locked Data Areas. Die gleiche
Projektion wird in `module_governance_map` und beim Release-Record-Sync
verwendet; runtime-installed Manifestpfade werden beim Release/Rollback ueber
den tatsaechlichen installierten App-Root aufgeloest. Dieser Slice ist Backend-
ready, aber nicht Phase-10-complete: Browser/App-Store/Settings muessen den
projizierten Zustand noch nach Reload sichtbar konsumieren, und Phase 15 muss
Operator-Recovery/Support-Drills fuer breitere Divergenzfaelle schliessen.
Phase 10E3 ist als UI/static-Teilslice begonnen: der gemeinsame Browser-
Lifecycle-Helper erzeugt jetzt eine Release-/Rollback-/Data-Access-Projektion
aus `lifecycle.release_state`, `lifecycle.rollback_target` und
`lifecycle.data_access`; App Store Cards/Details, der Shell-Lifecycle-Drawer
und Settings-Modulverwaltungszeilen rendern daraus business-faehige Freigabe-,
Rollback- und Datenbereich-Fakten. Historisch waren Reload-Browser-Smoke,
"success only after projection"-Evidence und Settings-Activity/browser-audit
zu diesem Zeitpunkt noch offen; diese Phase-10-Restpunkte sind inzwischen
durch `business-os-app-release-ui` geschlossen.
Die Production-Ready-Erweiterung fuer Phasen 10-16 wurde am 2026-06-17 erneut
gegen die aktuelle Source geprueft. Harte aktuelle Fakten: native
Release-/Rollback-Commands existieren in `store.rs` und werden vom
RxDB-Peer konsumiert; App Store und Settings dispatchen Release/Rollback ueber
`business_commands`; legacy HTTP-Handler fuer module source/release/rollback
sind aus `server.rs` entfernt und durch einen Source-Guard gegen Re-Enable
abgesichert. Der normale dynamische Modul-Kontext ruft seit Phase 13B
`createLiveDbFacade(mod)` mit aktivem Modul auf und hat Browser/Rust-Evidence
fuer Collection-, Property-, Cached-Handle- und Raw-Denial. Mehrere
Desktop-/Settings-/Chat-/Reporter-Pfade rufen `createLiveDbFacade()` weiterhin
ohne aktives Modul auf; die Phase-13B-Evidence ist deshalb noch kein Beweis
fuer vollstaendige Runtime-Isolation. Die Browser/Rust-Smoke-Matrix besitzt
Evidence- und Warning-Budget-Mechanik sowie registrierte Production-Modi fuer
Release, Audience, Agent Scope, Auth Scope und Fresh Profile; Release,
Audience und Agent Scope sind als echte Browser/Rust-Flows implementiert,
Auth Scope und Fresh Profile bleiben offene Browser-Stories.
Die Release-Workflows bauen Artefakte, haben aber noch keinen
Business-OS-Production-Gate-Job als Voraussetzung vor Upload/Release.
Die erneute Production-Ready-Erweiterung am 2026-06-17 ergaenzt deshalb nicht
nur weitere UI-Tasks, sondern alle produktionsrelevanten Ringe: native
Autoritaet, Business-UX, Daten-/Runtime-Isolation, Agentenscope, Auth/Tenant,
Offline-/Sync-/Concurrency-Verhalten, Audit/Support, Migration/Backup,
Performance/Scale, Security/Privacy und CI/Release-Gating. Source-gepruefte
harte Luecken: `app.js` importiert und mountet Module weiterhin im Shell-
Kontext; der dynamische Modul-Kontext ist nun guarded, aber mehrere
Desktop-/Settings-/Chat-/Reporter-Pfade rufen `createLiveDbFacade()` ohne
aktives Modul auf, App Store/Creator/Shell nutzen Browser Storage fuer
UI-Zustand, die Smoke-Matrix hat erst den Release-Production-Flow wirklich
implementiert, und `release.yml` laedt Business-OS-Artefakte hoch, ohne von
einem Business-OS-Production-Gate-Job abzuhaengen.

Validiert:

- Der Business-OS-Datenpfad ist RxDB/WebRTC-only; Browser-Commands laufen ueber
  `business_commands`, der Native Peer konsumiert pending Commands und schreibt
  Status zurueck.
- Persistente Rollen sind aktuell `chef`, `admin`, `founder`, `user`.
- `owner` wird backend- und frontendseitig zu `chef` normalisiert.
- Owner-Transfer ist seit Phase 3D/4E Owner-only: Admins und explizite
  `users.manage`-Grants koennen normale Nutzer-, Admin- und
  App-Verantwortliche-Rollen verwalten, aber keine Zielrolle `chef`/`owner`
  setzen.
- Deny UI nutzt seit Phase 4C/4D das akzeptierte mixed Pattern: Routine-
  Aktionen werden versteckt, erwartbare High-Value-Aktionen bleiben disabled
  mit `data-disabled-reason`, `title` und `aria-label`.
- App-Source-Zugriff ist seit Phase 3E/4F default hidden fuer Teammitglied:
  `ctox.source.load` und `ctox.source.list_snapshots` pruefen
  `apps.source.view`; Shell-Rechtsklick, Appbar und Source Editor filtern nach
  derselben Projektion. Exakte Grants erlauben Lesen, waehrend Speichern weiter
  `apps.modify` braucht.
- `team` und `business_os_team` sind seit Phase 0 explizit getestete
  Kompatibilitaets-Aliase zu `user`.
- `business_module_acl` bleibt ein founder-only Modul-ACL und keine generische
  Permission-Grant-Tabelle. Phase 3A fuehrt eine separate native, allow-only
  `business_permission_grants` Tabelle ein.
- App-/Modul-Governance-Commands sind ueber RxDB abgedeckt. Seit Phase 2
  werden app-build-shaped App create/modify Commands vor Queue-Erzeugung
  explizit policy-gegated.
- Phase 1 fuehrt einen zentralen Business-OS-Policy-Evaluator ein; bestehende
  Store-Wrapper fuer globale Verwaltung und Modulbearbeitung laufen darueber.
  Phase 2 verdrahtet den Evaluator in die wichtigsten nativen Control-Command-
  Pfade.
- Denied Commands schreiben fuer die Phase-2-Backend-Gates `status=failed`
  plus `result.policy_decision`. Seit Phase 6A schreiben native
  Policy-Denials zusaetzlich einen querybaren `business_events`-Eintrag mit
  Actor-Kontext und `policy_decision`.
- Seit Phase 6B schreiben `upsert_user` und `assign_module_founder` native
  Audit-Events in `business_events` mit Actor, vorherigem Zustand, aktuellem
  Zustand und `changed_fields`.
- Seit Phase 6D schreiben native Outbound-Freigabeentscheidungen eigene
  `business_events`-Eintraege mit Actor, Entscheidung, Approval-/Message-IDs
  und bewusst ohne Message-Body-Inhalt; die Settings-Activity rendert diese
  Ereignisse mit business-facing Labels.
- Seit Phase 6F schreiben erlaubte native Policy-Entscheidungen aus den
  bestehenden Gates `business_os.policy.allowed` in dieselbe
  `business_events`-Tabelle; `ctox.business_os.audit.list` wird fuer erlaubte
  Entscheidungen nicht selbst als Activity-Event geloggt.
- Phase 3A ist schema-stabil umgesetzt: keine neue RxDB Collection, kein
  Dist-Patch, keine Cache-Buster-Aenderung. Grants werden nativ gespeichert und
  als abgeleitetes `governance.permission_model` im bestehenden
  `business_module_catalog` projiziert.
- Phase 3B weitet Grant-Enforcement auf ausgewaehlte native Workspace-, Task-
  und App-Store-Command-Familien aus. Spaetere Phase-5-Slices haben MCP
  collection-, exact-record- und exact-approval-Scopes an dieselbe Policy
  gekoppelt; nicht-MCP Record-/Ownership-Ableitungen bleiben bewusst spaetere
  Produktarbeit.
- MCP hat weiterhin eigene Policy- und Audit-Guardrails, aber Phase
  5A/5B/5C/5D/5E koppeln die zentralen Action-/Approval-, Read-, MCP-Status-,
  Service-Actor- und Exact-Scope-Slices an die gemeinsame Business-OS-Policy.
- Phase 5A koppelt den MCP-Write-Slice an die gemeinsame Business-OS-Policy:
  `business_os.execute_action` prueft `data.write` auf dem Zielmodul,
  Approval-Tools pruefen `external.approve` auf `outbound`, bestehende MCP-
  Allowlists bleiben vorgelagert, und MCP-Audit-Metadaten enthalten fuer diese
  Tools kompatible `policy_decision` Felder.
- Phase 5B koppelt Modul-Detail-/Action-Proposal-Reads sowie
  collection-backed Record-Read-Pfade an `data.read`. Collection-Reads koennen
  durch einen exakten Collection-Grant oder ueber einen Modul-`data.read`-Grant
  erlaubt werden, wenn die Collection im Modul-Katalog gemappt ist. Feine
  Record-Ownership-Semantik bleibt neue Arbeit.
- Phase 5C koppelt `list_modules`, `open_link`, `get_command_status`,
  `status` und `list_mcp_activity` an die gemeinsame Policy:
  Modulauflistungen werden per Modul-`data.read` oder `data.write` gefiltert,
  Links pruefen je nach Linktyp Modul- oder Collection-`data.read`,
  Command-Status prueft `data.read` auf `business_commands`, und MCP-Status
  sowie MCP-Audit-Aktivitaet pruefen `mcp.manage` auf dem MCP-Scope.
- Phase 5D definiert MCP-Service-Actors ohne Schemaaenderung: persistierte
  `business_users` werden mit ihrer Rolle genutzt; unpersistierte MCP-Actor-IDs
  gelten als synthetische `user`-Service-Actors und brauchen exakte
  `business_permission_grants(subject_type=user, subject_id=<actor>)`.
  Command-Context und MCP-Audit enthalten die aufgeloeste Business-OS-Identitaet.
- Phase 5E fuehrt exakte MCP-Record- und Approval-Grants ein:
  `business_os.get_record` akzeptiert `data.read` auf
  `scope_type=record` mit `scope_id=<collection>/<record_id>`, ohne dadurch
  Collection-Listen/Suchen freizugeben; Approval-Tools akzeptieren
  `external.approve` auf `scope_type=approval` mit `scope_id=<approval_id>`,
  weiterhin mit Fallback auf den bestehenden `outbound`-Modulgrant.
- Phase 3F/5H schliesst die Ownership-Ableitungsgrenze fuer diesen Rollout:
  exakte Record-/Approval-Grants bleiben der freigeschaltete Object-Scope,
  waehrend automatische Ableitung aus freien Record-Payloads oder
  `outbound_approvals.actor_user_id` bewusst nicht implementiert ist.
- Phase 7 ergaenzt einen echten Full-App-Browser-Smoke fuer
  Rollen/Permissions: Teammitglied, exakter Source-Grant, exakter Modify-Grant,
  Owner, Owner-Transfer-Sichtbarkeit, Scope-Isolation, Reload und
  business-facing Labels werden im geladenen Business OS geprueft.
- Phase 8 ergaenzt einen echten Full-App-Browser-Smoke fuer dynamische Apps:
  Teammitglied sieht private `0.x` Apps nicht, App-Verantwortliche sehen sie,
  `1.0.0` Apps sind Team-default sichtbar, explizit eingeschraenkte Team-Apps
  sind fuer Team verborgen, Lifecycle-Badges und Drawer rendern, und
  runtime-installed Apps koennen `ctx.db`/`ctx.db.raw` ohne `data.read`/
  `data.write` nicht umgehen.
- Phase 10A ergaenzt native Release-/Rollback-Command-Gates: Rollback nutzt
  `apps.rollback` end-to-end, fehlgeschlagene Rollback- und Release-Pruefungen
  schreiben failed command outcomes, Versionen-Listing ist backendseitig
  source/release/rollback-gegated, und Data-Review-Evidence erzeugt keine
  impliziten Datenrechte.
- Phase 10B ergaenzt native Data-Review-Reconciliation: Team-Releases
  akzeptieren reviewed read/write Collections nur mit expliziten Team-
  Datengrants oder einer deklarierten Locked-State-Behandlung; der
  Release-Snapshot enthaelt die granted/locked Evidence und bleibt
  evidence-only.
- Phase 10C entfernt die toten legacy HTTP module source/release/rollback
  Server-Handler und ratchet den RxDB-only Guard so, dass Route-Strings und
  direkte Server-Helper/Store-Marker nicht wieder eingefuehrt werden koennen.
- Phase 10E2 ergaenzt native Release-State-Projektion: Catalog-Lifecycle und
  Governance-Map enthalten aktuelle Release-Version, Release-Historie,
  Rollback-Ziel und Data-Access-Summary mit granted/locked Data Areas.
  Runtime-installed Release/Rollback nutzt den tatsaechlichen installierten
  Manifestpfad statt eines falschen Fallback-App-Roots.

Source-validated reference points:

| Area | Current source |
| --- | --- |
| Role normalization | `src/core/business_os/store.rs`, `src/apps/business-os/app.js`, `src/apps/business-os/shared/react-settings.js` |
| User/module ACL persistence | `src/core/business_os/store.rs` |
| Native command consumption | `src/core/business_os/rxdb_peer.rs` |
| Browser command dispatch | `src/apps/business-os/shared/command-bus.js` |
| Settings and role UI | `src/apps/business-os/shared/react-settings.js` |
| Context menu app modify | `src/apps/business-os/app.js` and module chat controls |
| App Store actions | `src/apps/business-os/modules/app-store/index.js` |
| Dynamic app lifecycle and guarded browser DB facade | `src/apps/business-os/shared/app-lifecycle.js`, `src/apps/business-os/app.js`, `src/apps/business-os/modules/app-store/index.js`, `src/core/rxdb/tools/browser_rust_smoke.js` |
| RxDB schemas and hashes | `src/apps/business-os/modules/ctox/schema.js`, `src/core/business_os/business_os_schema_contract.json`, `src/core/business_os/business_os_schema_hashes.json`, `src/apps/business-os/rxdb/src/schema.mjs` |
| Native/MCP policy audit | `src/core/business_os/store.rs`, `src/core/business_os/mcp_channel.rs`, `business_events`, `business_os_mcp_events` |

Full production-readiness extension anchors:

| Area | Current source |
| --- | --- |
| Native app release and rollback commands | `src/core/business_os/store.rs:3026`, `src/core/business_os/store.rs:3100`, `src/core/business_os/store.rs:8306`, `src/core/business_os/store.rs:8449`, `src/core/business_os/rxdb_peer.rs:2617` |
| Module version timeline and source rollback | `src/core/business_os/store.rs:4612`, `src/core/business_os/store.rs:4706`, `src/core/business_os/store.rs:4788`, `src/core/business_os/store.rs:8265`, `src/apps/business-os/app.js:1362` |
| Release lifecycle projection and runtime-installed manifest path consistency | `src/core/business_os/store.rs:2035`, `src/core/business_os/store.rs:2107`, `src/core/business_os/store.rs:2349`, `src/core/business_os/store.rs:2380`, `src/core/business_os/store.rs:2489`, `src/core/business_os/store.rs:6305`, `src/core/business_os/store.rs:6332`, `src/core/business_os/store.rs:30220` |
| App Store release/rollback affordances | `src/apps/business-os/modules/app-store/index.js:838`, `src/apps/business-os/modules/app-store/index.js:935`, `src/apps/business-os/modules/app-store/app-store.test.mjs` |
| Settings release/rollback read-only diagnostics | `src/apps/business-os/shared/react-settings.js:395`, `src/apps/business-os/shared/react-settings.js:955`, `src/apps/business-os/shared/react-settings.test.mjs:153` |
| App Creator generated app version default | `src/core/business_os/store.rs:3934`, `src/core/business_os/store.rs:3998`, `src/apps/business-os/modules/creator/index.js:1475` |
| Runtime app browser context and guarded DB facade | `src/apps/business-os/app.js:1731`, `src/apps/business-os/app.js:4099`, `src/apps/business-os/app.js:4717` |
| Native "Warum?" diagnostics command | `src/core/business_os/store.rs::BusinessOsWhyDiagnosticsRequest`, `src/core/business_os/store.rs::business_os_why_diagnostics`, `src/core/business_os/store.rs::business_os_why_visibility_decision`, `src/core/business_os/store.rs::business_os_why_data_permission_decision`, `src/core/business_os/store.rs::accept_rxdb_business_command` |
| Settings "Warum?" diagnostics UI | `src/apps/business-os/shared/react-settings.js::loadModuleWhyDiagnostics`, `src/apps/business-os/shared/react-settings.js::nativeWhyDiagnosticsView`, `src/apps/business-os/shared/react-settings.js::moduleWhyDiagnosticsHtml`, `src/apps/business-os/shared/react-settings.test.mjs`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsRolesPermissionsUiSmoke` |
| Native support diagnostics artifact | `src/core/business_os/store.rs::BusinessSupportDiagnosticsExportRequest`, `src/core/business_os/store.rs::business_os_support_diagnostics_export`, `src/core/business_os/store.rs::support_diagnostics_activity_summary`, `src/core/business_os/store.rs::support_diagnostics_why_summary`, `src/core/business_os/store.rs::accept_rxdb_business_command` |
| Business OS auth/session and logout surfaces | `src/core/business_os/server.rs:298`, `src/core/business_os/server.rs:712`, `src/core/business_os/server.rs:2806`, `src/apps/business-os/app.js:5017`, `src/apps/business-os/app.js:5244` |
| WebRTC-only sync and no HTTP bridge | `README.md:54`, `README.md:165`, `docs/ctox-rxdb.md:76`, `src/core/business_os/store.rs:795`, `src/apps/business-os/shared/sync.js:26` |
| Browser smoke evidence and diagnostics budgets | `src/core/rxdb/tools/browser_rust_smoke.js:3926`, `src/core/rxdb/tools/browser_rust_smoke.js:7044`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js:206`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js:854` |
| Existing module test surface for app-by-app migration | `src/apps/business-os/scripts/assert-module-conformance.mjs`, `src/apps/business-os/modules/*/*.test.mjs`, `src/apps/business-os/desktop-apps/code-editor/code-editor.test.mjs` |

Hard source anchors used for this revision:

| Finding | Source anchors |
| --- | --- |
| Role aliases and labels | `src/core/business_os/policy.rs:245`, `src/core/business_os/store.rs:1153`, `src/core/business_os/store.rs:17999`, `src/apps/business-os/shared/roles.js:1`, `src/apps/business-os/shared/roles.test.mjs:12` |
| Owner-only user role transfer | `src/core/business_os/store.rs:2046`, `src/core/business_os/store.rs:5776`, `src/core/business_os/store.rs:7911`, `src/core/business_os/store.rs:18653`, `src/core/business_os/store.rs:18694`, `src/core/business_os/store.rs:18746`, `src/apps/business-os/shared/roles.js:33`, `src/apps/business-os/shared/react-settings.js:779`, `src/apps/business-os/shared/roles.test.mjs:48`, `src/apps/business-os/shared/react-settings.test.mjs:57` |
| Central policy evaluator | `src/core/business_os/policy.rs:25`, `src/core/business_os/policy.rs:220`, `src/core/business_os/policy.rs:245`, `src/core/business_os/store.rs:1180`, `src/core/business_os/store.rs:1201` |
| Trusted role comes from persisted users | `src/core/business_os/store.rs:12600`, `src/core/business_os/store.rs:12689`, `src/core/business_os/store.rs:20837` |
| App create/modify backend gate before queueing | `src/core/business_os/store.rs:5348`, `src/core/business_os/store.rs:5390`, `src/core/business_os/store.rs:18540` |
| Native control-command policy catalog | `src/core/business_os/store.rs:7481`, `src/core/business_os/store.rs:7552`, `src/core/business_os/store.rs:7793`, `src/core/business_os/store.rs:7983`, `src/core/business_os/store.rs:8110`, `src/core/business_os/store.rs:18738` |
| Structured denied-command output | `src/core/business_os/store.rs:5377`, `src/core/business_os/store.rs:5400`, `src/core/business_os/store.rs:18033`, `src/core/business_os/store.rs:18817` |
| App Store admin install/uninstall allow path | `src/core/business_os/store.rs:18971` |
| Phase 6A/6B native audit and MCP audit stores | `src/core/business_os/store.rs:2084`, `src/core/business_os/store.rs:2131`, `src/core/business_os/store.rs:5447`, `src/core/business_os/store.rs:5466`, `src/core/business_os/store.rs:5606`, `src/core/business_os/store.rs:5639`, `src/core/business_os/store.rs:19376`, `src/core/business_os/store.rs:19454`, `src/core/business_os/mcp_channel.rs:2648`, `src/core/business_os/mcp_channel.rs:2720` |
| Phase 6C Activity command and Settings UI | `src/core/business_os/store.rs:185`, `src/core/business_os/store.rs:5496`, `src/core/business_os/store.rs:7889`, `src/core/business_os/store.rs:19616`, `src/core/business_os/store.rs:19734`, `src/apps/business-os/shared/react-settings.js:37`, `src/apps/business-os/shared/react-settings.js:141`, `src/apps/business-os/shared/react-settings.js:559`, `src/apps/business-os/shared/react-settings.js:786`, `src/apps/business-os/shared/react-settings.js:1399`, `src/apps/business-os/shared/react-settings.test.mjs:65` |
| Phase 6D Outbound approval decision activity | `src/core/business_os/store.rs:5509`, `src/core/business_os/store.rs:9377`, `src/core/business_os/store.rs:12320`, `src/core/business_os/store.rs:12345`, `src/core/business_os/store.rs:12429`, `src/core/business_os/store.rs:12483`, `src/core/business_os/store.rs:19838`, `src/apps/business-os/shared/react-settings.js:834`, `src/apps/business-os/shared/react-settings.js:865`, `src/apps/business-os/shared/react-settings.js:873`, `src/apps/business-os/shared/react-settings.test.mjs:105`, `src/apps/business-os/shared/react-settings.test.mjs:132` |
| Phase 6E rollout and recovery guidance | `docs/business-os-roles-permissions-rollout.md:1`, `docs/business-os-roles-permissions-rollout.md:40`, `docs/business-os-roles-permissions-rollout.md:54`, `docs/business-os-roles-permissions-rollout.md:97`, `docs/business-os-roles-permissions-rollout.md:126` |
| Phase 6F/6G allowed and denied policy-decision activity | `src/core/business_os/store.rs:5369`, `src/core/business_os/store.rs:5389`, `src/core/business_os/store.rs:5445`, `src/core/business_os/store.rs:5502`, `src/core/business_os/store.rs:5743`, `src/core/business_os/store.rs:5749`, `src/core/business_os/store.rs:5759`, `src/core/business_os/store.rs:19554`, `src/core/business_os/store.rs:19947`, `src/core/business_os/store.rs:19978`, `src/apps/business-os/shared/react-settings.js:831`, `src/apps/business-os/shared/react-settings.js:842`, `src/apps/business-os/shared/react-settings.test.mjs:105`, `src/apps/business-os/shared/react-settings.test.mjs:145` |
| RxDB/WebRTC-only and no HTTP data bridge | `src/core/business_os/store.rs:789`, `src/core/business_os/store.rs:812`, `docs/ctox-rxdb.md:82`, `src/core/business_os/server.rs:229`, `src/core/business_os/server.rs:696` |
| Shell right-click app modification is role/module aware | `src/apps/business-os/app.js:3281`, `src/apps/business-os/app.js:5013`, `src/apps/business-os/app.js:7416` |
| App Store selected-app modify and role-gated affordances were missing before Phase 4A/4C | `src/apps/business-os/modules/app-store/index.js:674`, `src/apps/business-os/modules/app-store/index.js:1212`, `src/apps/business-os/modules/app-store/index.js:1503`, `src/apps/business-os/modules/app-store/index.js:1631`, `src/apps/business-os/modules/app-store/app-store.test.mjs:122`, `src/apps/business-os/modules/app-store/app-store.test.mjs:184` |
| Settings legacy labels were present before Phase 4B touched-label cleanup | `src/apps/business-os/shared/react-settings.js:509`, `src/apps/business-os/shared/react-settings.js:712`, `src/apps/business-os/shared/react-settings.test.mjs:43`, `src/apps/business-os/shared/react-settings.test.mjs:56` |
| MCP is still separately policy-configured | `src/core/business_os/mcp_channel.rs:2050`, `src/core/business_os/mcp_channel.rs:2130`, `src/core/business_os/mcp_channel.rs:3480` |
| Phase 3A permission catalog and grant-aware module decision | `src/core/business_os/policy.rs:46`, `src/core/business_os/policy.rs:69`, `src/core/business_os/policy.rs:220`, `src/core/business_os/store.rs:1168`, `src/core/business_os/store.rs:1245` |
| Phase 3A native grants and derived projection | `src/core/business_os/store.rs:1378`, `src/core/business_os/store.rs:1472`, `src/core/business_os/store.rs:1494`, `src/core/business_os/store.rs:17508` |
| Phase 3A tests | `src/core/business_os/store.rs:18011`, `src/core/business_os/store.rs:18049` |
| Phase 3B grant-aware command routing | `src/core/business_os/store.rs:1180`, `src/core/business_os/store.rs:1201`, `src/core/business_os/store.rs:5425`, `src/core/business_os/store.rs:5433`, `src/core/business_os/store.rs:7552`, `src/core/business_os/store.rs:7793`, `src/core/business_os/store.rs:7983` |
| Phase 3B tests | `src/core/business_os/store.rs:18117`, `src/core/business_os/store.rs:18172`, `src/core/business_os/store.rs:18234` |
| Phase 3C source-validated task ownership boundary | `src/core/business_os/policy.rs:277`, `src/core/business_os/store.rs:1298`, `src/core/business_os/store.rs:5771`, `src/core/mission/channels.rs:57`, `src/core/mission/channels.rs:2735`, `src/core/business_os/store.rs:16198`, `src/core/business_os/store.rs:18627` |
| Phase 4A shared browser permission helper | `src/apps/business-os/shared/permissions.js:3`, `src/apps/business-os/shared/permissions.js:47`, `src/apps/business-os/shared/permissions.js:82`, `src/apps/business-os/shared/permissions.test.mjs:76`, `src/apps/business-os/shared/permissions.test.mjs:114` |
| Phase 4A Shell and Settings helper consumption | `src/apps/business-os/app.js:5014`, `src/apps/business-os/shared/react-settings.js:134`, `src/apps/business-os/shared/react-settings.js:712`, `src/apps/business-os/shared/react-settings.js:1143`, `src/apps/business-os/shared/react-settings.js:1150` |
| Phase 4A App Store affordances and selected-app modify target | `src/apps/business-os/modules/app-store/index.js:674`, `src/apps/business-os/modules/app-store/index.js:1212`, `src/apps/business-os/modules/app-store/index.js:1503`, `src/apps/business-os/modules/app-store/index.js:1631`, `src/apps/business-os/modules/app-store/app-store.test.mjs:122`, `src/apps/business-os/modules/app-store/app-store.test.mjs:175` |
| Phase 4B UI permission guard and Settings render labels | `src/apps/business-os/scripts/assert-permissions-ui.mjs:17`, `src/apps/business-os/scripts/assert-permissions-ui.mjs:32`, `src/apps/business-os/shared/react-settings.js:509`, `src/apps/business-os/shared/react-settings.test.mjs:43`, `src/apps/business-os/shared/react-settings.test.mjs:56` |
| Phase 4C disabled App Store reasons and affordance refresh | `src/apps/business-os/modules/app-store/index.js:677`, `src/apps/business-os/modules/app-store/index.js:1221`, `src/apps/business-os/modules/app-store/index.js:1243`, `src/apps/business-os/modules/app-store/index.css:809`, `src/apps/business-os/modules/app-store/app-store.test.mjs:122`, `src/apps/business-os/modules/app-store/app-store.test.mjs:184` |
| Phase 4D shell right-click render and app-change wording | `src/apps/business-os/shared/shell-permissions-ui.js:1`, `src/apps/business-os/shared/shell-permissions-ui.js:44`, `src/apps/business-os/shared/shell-permissions-ui.js:70`, `src/apps/business-os/shared/shell-permissions-ui.test.mjs:87`, `src/apps/business-os/shared/shell-permissions-ui.test.mjs:112`, `src/apps/business-os/app.js:3368`, `src/apps/business-os/app.js:7435`, `src/apps/business-os/scripts/assert-permissions-ui.mjs:43`, `src/apps/business-os/modules/app-store/index.js:1618` |
| Phase 4E Deny UI accepted mixed pattern | `src/apps/business-os/shared/shell-permissions-ui.js:33`, `src/apps/business-os/shared/shell-permissions-ui.js:60`, `src/apps/business-os/shared/shell-permissions-ui.test.mjs:91`, `src/apps/business-os/shared/shell-permissions-ui.test.mjs:113`, `src/apps/business-os/modules/app-store/index.js:1218`, `src/apps/business-os/modules/app-store/index.js:1227`, `src/apps/business-os/modules/app-store/index.js:1231`, `src/apps/business-os/modules/app-store/index.js:1243`, `src/apps/business-os/modules/app-store/index.js:1248`, `src/apps/business-os/modules/app-store/app-store.test.mjs:122`, `src/apps/business-os/modules/app-store/app-store.test.mjs:184` |
| Phase 3E/4F App source visibility | `src/core/business_os/store.rs:8156`, `src/core/business_os/store.rs:8208`, `src/core/business_os/store.rs:18677`, `src/apps/business-os/shared/permissions.js:93`, `src/apps/business-os/shared/shell-permissions-ui.js:30`, `src/apps/business-os/shared/shell-permissions-ui.js:49`, `src/apps/business-os/app.js:1596`, `src/apps/business-os/app.js:3036`, `src/apps/business-os/app.js:4131`, `src/apps/business-os/app.js:5035`, `src/apps/business-os/desktop-apps/code-editor/app.js:200`, `src/apps/business-os/desktop-apps/code-editor/app.js:249`, `src/apps/business-os/desktop-apps/code-editor/app.js:664`, `src/apps/business-os/desktop-apps/code-editor/app.js:762`, `src/apps/business-os/shared/permissions.test.mjs:117`, `src/apps/business-os/shared/shell-permissions-ui.test.mjs:135`, `src/apps/business-os/shared/shell-permissions-ui.test.mjs:154`, `src/apps/business-os/desktop-apps/code-editor/code-editor.test.mjs:41`, `src/apps/business-os/desktop-apps/code-editor/code-editor.test.mjs:131` |
| Phase 5A/5B/5C/5D/5E MCP shared policy gates, service actors, exact scopes and audit metadata | `src/core/business_os/store.rs:151`, `src/core/business_os/store.rs:1212`, `src/core/business_os/store.rs:1232`, `src/core/business_os/store.rs:1241`, `src/core/business_os/store.rs:12689`, `src/core/business_os/mcp_channel.rs:1434`, `src/core/business_os/mcp_channel.rs:1692`, `src/core/business_os/mcp_channel.rs:1716`, `src/core/business_os/mcp_channel.rs:1745`, `src/core/business_os/mcp_channel.rs:2185`, `src/core/business_os/mcp_channel.rs:2227`, `src/core/business_os/mcp_channel.rs:2337`, `src/core/business_os/mcp_channel.rs:2364`, `src/core/business_os/mcp_channel.rs:2406`, `src/core/business_os/mcp_channel.rs:2477`, `src/core/business_os/mcp_channel.rs:2634`, `src/core/business_os/mcp_channel.rs:2711`, `src/core/business_os/mcp_channel.rs:4038`, `src/core/business_os/mcp_channel.rs:4063`, `src/core/business_os/mcp_channel.rs:4147`, `src/core/business_os/mcp_channel.rs:4347` |
| Phase 5F source-validated decision boundaries | `src/core/business_os/store.rs:1281`, `src/core/business_os/store.rs:1298`, `src/core/business_os/store.rs:1301`, `src/core/business_os/store.rs:1359`, `src/core/business_os/store.rs:1375`, `src/core/business_os/policy.rs:272`, `src/core/business_os/policy.rs:277`, `src/core/business_os/policy.rs:282`, `src/core/business_os/mcp_channel.rs:1723`, `src/core/business_os/mcp_channel.rs:1736`, `src/core/business_os/mcp_channel.rs:2061`, `src/core/business_os/mcp_channel.rs:2141`, `src/core/business_os/mcp_channel.rs:2274`, `src/core/business_os/mcp_channel.rs:2349`, `src/core/business_os/mcp_channel.rs:2375`, `src/core/business_os/mcp_channel.rs:2625`, `src/core/business_os/mcp_channel.rs:3919`, `src/core/business_os/mcp_channel.rs:4829` |
| Phase 5G accepted MCP external-effect v1 boundary | `src/core/business_os/mcp_channel.rs:975`, `src/core/business_os/mcp_channel.rs:1717`, `src/core/business_os/mcp_channel.rs:1736`, `src/core/business_os/mcp_channel.rs:2061`, `src/core/business_os/mcp_channel.rs:2073`, `src/core/business_os/mcp_channel.rs:2108`, `src/core/business_os/mcp_channel.rs:2149`, `src/core/business_os/mcp_channel.rs:2625`, `src/core/business_os/mcp_channel.rs:2627`, `src/core/business_os/mcp_channel.rs:3919`, `src/core/business_os/mcp_channel.rs:4891`, `src/core/business_os/mcp_channel.rs:5445`, `src/core/business_os/mcp_channel.rs:5504` |
| Phase 3F/5H accepted exact-grants-only ownership boundary | `src/core/business_os/policy.rs:272`, `src/core/business_os/policy.rs:282`, `src/core/business_os/store.rs:1307`, `src/core/business_os/store.rs:1375`, `src/core/business_os/store.rs:9348`, `src/core/business_os/store.rs:9412`, `src/core/business_os/store.rs:18554`, `src/core/business_os/mcp_channel.rs:2227`, `src/core/business_os/mcp_channel.rs:2258`, `src/core/business_os/mcp_channel.rs:2343`, `src/core/business_os/mcp_channel.rs:2374`, `src/core/business_os/mcp_channel.rs:4272`, `src/core/business_os/mcp_channel.rs:4522` |

Plan language uses these meanings:

- `Current source`: behavior that exists today and was found in code.
- `New work`: planned implementation work that does not exist yet.
- `Decision required`: a product or architecture choice that must be closed
  before implementation for that phase starts.

## Operating Constraints

- Business-OS-Daten, Records, Commands, Module, Rechteprojektionen und Runtime
  State bleiben auf dem CTOX DB / RxDB / WebRTC Datenpfad.
- Es wird kein HTTP-Fallback und kein HTTP-RxDB-Proxy fuer Business-OS-Daten
  eingefuehrt.
- Browser- oder Client-Rollen sind nie autoritativ. Der Native Peer / Store
  muss Actor, Rolle und Grants aus persistierten Business-OS-Daten ableiten.
- `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs` wird nicht direkt gepatcht.
- Schema- und Wire-Contract-Aenderungen muessen aus Fixtures/Generatoren
  entstehen und die JS/Rust-Hashes synchron halten.
- Neue Laufzeit-Schalter duerfen nicht ueber Prozess-Env-Vars eingefuehrt
  werden. Laufzeitkonfiguration gehoert in typisierte persistierte Config.
- Bestehende Dirty-Worktree-Aenderungen anderer Arbeit werden nicht reverted.

## Product Model

### Role Labels

Current persisted roles remain `chef`, `admin`, `founder`, `user` until a
separate migration phase changes that. UI labels can change earlier, but storage
compatibility must remain intact.

| UI Label | Current / Planned Alias | Business Meaning |
| --- | --- | --- |
| `Owner` / `Besitzer` | `chef`, `owner` | Verantwortet die Instanz, kritische Einstellungen, Rollen, App-Installation, Integrationen, Agentenzugriff und Recovery. |
| `Admin` | `admin` | Betreibt Business OS im Alltag, verwaltet Nutzer, Apps, Zuweisungen und Einstellungen ohne Owner-Transfer. |
| `App-Verantwortliche:r` | `founder` | Darf die zugewiesenen Apps fachlich aendern, releasen, zurueckrollen und per Rechtsklick modifizieren. |
| `Teammitglied` | `user`; alias `team` | Nutzt freigegebene Apps und Daten, darf keine Apps, Rollen oder Policies aendern. |

`Founder` bleibt zunaechst als interner Kompatibilitaetswert moeglich, soll in
der UI aber durch `App-Verantwortliche:r` ersetzt werden.

### Permission Vocabulary

Die folgende Permission-Liste ist das Zielmodell, nicht der aktuelle Source-
Zustand. Heute gibt es verstreute Rollen- und Modul-ACL-Checks; Phase 1
fuehrt erst den zentralen Permission-Katalog ein. Die UI soll keine kryptischen
Capability-Namen zeigen. Interne Permissions duerfen technisch sein, aber sie
werden in Business-Verben uebersetzt:

| Internal Permission | UI Wording |
| --- | --- |
| `workspace.manage` | Business OS verwalten |
| `users.manage` | Team und Zugaenge verwalten |
| `roles.manage` | Rollen und Sonderrechte vergeben |
| `runtime.manage` | System-Einstellungen aendern |
| `integrations.manage` | Kanaele und Integrationen verbinden |
| `mcp.manage` | Agentenzugriff verwalten |
| `apps.install` | Apps installieren |
| `apps.uninstall` | Apps entfernen |
| `apps.assign_owner` | App-Verantwortliche festlegen |
| `apps.modify` | App aendern |
| `apps.release` | App veroeffentlichen |
| `apps.rollback` | App-Version zurueckrollen |
| `apps.source.view` | App-Quellen ansehen |
| `data.read` | Daten ansehen |
| `data.write` | Daten bearbeiten |
| `ctox.task.create` | CTOX Aufgabe starten |
| `ctox.task.manage` | CTOX Aufgaben steuern |
| `external.approve` | Externe Wirkung freigeben |

### Default Role Matrix

Die Matrix ist ein Zielbild fuer Phase 1/2. Nur die groben heutigen
Entsprechungen `chef/admin` global und `founder` fuer zugewiesene Module sind
bereits im Source vorhanden.

| Permission | Owner | Admin | App-Verantwortliche:r | Teammitglied |
| --- | --- | --- | --- | --- |
| `workspace.manage` | yes | no | no | no |
| `users.manage` | yes | yes | no | no |
| `roles.manage` | yes | limited | no | no |
| `runtime.manage` | yes | yes | no | no |
| `integrations.manage` | yes | yes | no | no |
| `mcp.manage` | yes | yes | no | no |
| `apps.install` | yes | yes | no | no |
| `apps.uninstall` | yes | yes | no | no |
| `apps.assign_owner` | yes | yes | no | no |
| `apps.modify` | yes | yes | assigned app only | no |
| `apps.release` | yes | yes | assigned app only | no |
| `apps.rollback` | yes | yes | assigned app only | no |
| `apps.source.view` | yes | yes | assigned app only | optional by app |
| `data.read` | yes | yes | assigned app data | assigned app data |
| `data.write` | yes | yes | assigned app data | assigned app data |
| `ctox.task.create` | yes | yes | yes | yes |
| `ctox.task.manage` | yes | yes | planned owned or assigned; current exact grant | planned owned; current exact grant |
| `external.approve` | yes | yes | explicit grant only | explicit grant only |

## Target Architecture

### Decision Flow

Target flow after Phases 1-4. Today the browser still duplicates role logic in
several modules. Since Phase 2, the main app/admin command gates write
structured policy decisions; Phase 3A adds the native permission model
projection, Phase 4 consumes it in the browser UI, and general audit remains
Phase 6 work.

```text
Business OS UI
  -> permission/effective-permission projection from RxDB
  -> user sees only allowed actions or disabled actions with plain reason
  -> business_commands document
  -> Native Peer consumes pending command
  -> Store loads trusted actor, role, grants, object scope
  -> Policy evaluator returns allow/deny with reason
  -> Command is accepted or rejected with auditable decision
```

### Policy Inputs

| Input | Source of Truth |
| --- | --- |
| Actor id | Command context, then resolved against `business_users` |
| Actor role | Persisted `business_users.role` |
| Module ownership | Current source: founder-only `business_module_acl`; later generic permission grants |
| Explicit grants | Current source after Phase 3A: native allow-only `business_permission_grants` table for exact scoped grants; no first-class RxDB grant collection yet |
| Object ownership | New work beyond current module founder ownership, task-scope policy hooks and exact scoped grants; queue-task `lease_owner` is runtime state, not Business OS ownership |
| Risk class | New work; command/action catalog must be introduced |
| Runtime policy | Persisted typed Business OS config, not process env |

### Policy Outputs

Phase 1 introduced an in-process `PolicyDecision` shape for the evaluator.
Phase 2 persists stable reason codes and display reasons for the main
app/admin command gates in `business_commands.result.policy_decision`. Phase
6A/6F persist native denied and allowed policy decisions in `business_events`
for support and Activity UI visibility; MCP keeps its dedicated
`business_os_mcp_events` audit stream.

| Field | Meaning |
| --- | --- |
| `allowed` | Boolean policy result |
| `permission` | Evaluated internal permission |
| `scope_type` | `workspace`, `module`, `collection`, `record`, `task`, `approval`, `mcp` |
| `scope_id` | Concrete module id, record id, task id, etc. |
| `reason_code` | Stable reason for logs/tests |
| `display_reason` | Plain-language UI reason |
| `requires_approval` | Whether the action can continue after approval |
| `audit_level` | `none`, `decision`, `security`, `external_effect` |

## Phase Tracker

| Phase | Title | Status | Owner | Evidence |
| --- | --- | --- | --- | --- |
| 0 | Baseline and naming contract | Complete | Codex | Shared role helper + backend alias implemented; Phase 0 gates pass |
| 1 | Central policy evaluator | Complete | Codex | Rust policy core added; existing wrappers routed through policy; `business_os` gate passes |
| 2 | Command enforcement hardening | Complete | Codex | App create/modify, task manage, App Store allow/deny and main native control-command gates implemented; `business_os` gate passes |
| 3 | Permission grants and projection | Complete | Codex | Phase 3A/3B/3E/3F gates pass: native allow-only grants, catalog projection, module grants plus selected workspace/task/App Store grant routing, source-load/snapshot-list source-view gates, and accepted exact-grants-only record ownership boundary; non-MCP ownership derivation is deferred to a future normalized product model |
| 4 | Business-facing UI | Complete | Codex | Phase 4A/4B/4C/4D/4F gates plus Owner/Teammitglied static Playwright browser smokes pass; source affordances and Source Editor consume projected source-view grants |
| 5 | MCP and external-agent alignment | Complete | Codex | Phase 5A/5B/5C/5D/5E/5F/5G/5H gates pass: `execute_action`, approval decision routing, module detail/action-proposal reads, collection-backed record reads, exact `get_record` grants, exact approval grants, module listing, scoped links, command status, MCP status/activity and service-actor mapping route through shared policy; existing allowlists remain stricter filters; MCP external effects stay blocked for MCP Channel v1/current rollout; automatic record/approval ownership derivation is deferred to a future normalized product model |
| 6 | Migration, audit, rollout | Complete | Codex | Phase 6A/6B/6C/6D/6E/6F/6G gates pass; native denied/allowed policy decisions, role/app-responsibility changes, Outbound approval decisions, Activity UI, rollout docs, plan coherence and broad release gates are covered; audit retention/volume tuning is a future product decision, not a Phase 6 blocker |
| 7 | Production-readiness hardening | Complete | Codex | Live Browser/Rust role-permission smoke and UI-regression matrix pass; wire-daemon skip evidence eliminated; broad RxDB/Rust/JS release gates pass locally |
| 8 | Dynamic app lifecycle, visibility and browser data isolation | Complete | Codex | Core slice implemented and locally verified: shared lifecycle helper/tests, Shell/App-Store lifecycle UI, restricted/team/private visibility semantics, guarded runtime-installed DB helper/smoke-hook path, and Browser/Rust dynamic-app smoke matrix pass; later Phase 13B evidence closes the real `createModuleContext(mod)` and persisted `openModule`/reload guarded-facade path, while packaged/core facades and broader production hardening remain in Phases 13-16 |
| 9 | Native lifecycle projection and migration | Complete | Codex | Native module catalog now projects lifecycle/release metadata through existing `business_module_catalog.governance.lifecycle`, with Rust backfill/projection tests, JS projected-metadata tests and Browser/Rust reload smoke evidence; no new RxDB collection or HTTP fallback |
| 10 | App Store publish and data-access review flow | Complete | Codex | Phase 10A/10B/10C/10D1/10D2/10E1/10E2 backend/static gates pass: rollback `apps.rollback` enforcement, failed rollback/release outcomes, backend-gated `list_versions`, evidence-only data-review subset checks, Team data grant/locked-state reconciliation, no legacy HTTP module source/release/rollback handlers in `server.rs`, stale release-version ref rejection before manifest write, manifest restore on injected release/rollback DB failures, duplicate release command idempotency, tested lifecycle projection repair, queryable native `business_events` for successful/failed release and rollback outcomes including `ctox.module.rollback_version`, and native catalog/governance projection of release state, rollback target and granted/locked data areas. Phase 10E3 UI/static renders projected release/rollback/data-area facts in Shell, App Store and Settings; Phase 10F adds the App Store release button/dialog/payload guarded by `apps.release`; Phase 10G Settings release/rollback is Browser/Rust-proven read-only diagnostics. Live Browser/Rust `business-os-app-release-ui` proves private `0.x` visibility, App Store release to Team `1.0.0`, data-review display, projected version badge, reload, storage-boundary, rollback, Settings Activity release/rollback audit rows and redacted Activity labels with browser errors/404/request failures 0. `P10-UI-WIZARD`, `P10-BE-RELEASE-PROJECTION`, `P10-SETTINGS-FALLBACK` and `P10-BE-AUDIT` are closed |
| 11 | Audience, preview and responsibility management | Complete | Codex | 11A/11B/11C/11D are implemented and verified: native/browser `apps.view`, Modify-only no longer grants non-public visibility, durable preview grants, hidden deep-link route lock, Browser/Rust audience smoke, orphan-safe App-Verantwortliche responsibility for private runtime apps, launcher/start-menu lifecycle badges, and permission-aware lifecycle drawer states for manager vs Team read-only actors |
| 12 | Agent and app-scope UX parity | Complete | Codex | Phase 12A backend MCP visibility/data split is implemented and verified: MCP module listing, detail/action/proposal policy, module links and execution evaluate app lifecycle/audience visibility before `data.read`/`data.write`; visible-app/data-denied and hidden-app/data-granted cases are covered. Phase 12B/12C browser-side slices are locally verified for the global right-click CTOX menu, App Store context chat and Business Chat rendering: visible actor/app/selection/data/external scope is rendered from the same object that is submitted as `client_context.visible_scope`, the command bus canonicalizes module/app/action/mode/target/record/scope aliases without overwriting caller actors, Coding Agents add provider/workspace/session external-scope context, App Store context chat builds/submits selected-app `visible_scope`, Business Chat renders preserved visible scope rows in the chat window, and scheduled Business Chat commands preserve existing `contextMeta.client_context`. Live Browser/Rust `business-os-agent-scope-ui` proves the global CTOX context menu, App Store context menu and Business Chat rendered scope match submitted `client_context.visible_scope`; hidden private app open is denied, data read is denied before and allowed after an explicit grant, write stays denied without `data.write`, persisted command audit keeps visible scope, and Settings renders active Sonderfreigaben as a read-only Owner/Admin boundary. Native/MCP audit metadata parity is now locally verified: native policy events persist redacted scope-only client_context, MCP events persist business_scope, and targeted Rust tests prove free-form prompts, selections and MCP payloads are not copied into audit metadata |
| 13 | Packaged/core module data-isolation migration | Complete | Codex | Phase 13A inventory is implemented and guarded: 24 modules, 5 desktop apps and the originally unscoped Shell/Desktop facades are classified with owner/review status in `docs/business-os-db-isolation-inventory.json`. Phase 13B is closed: normal dynamic `createModuleContext(mod)` passes the active module into `createLiveDbFacade(mod)`, and Browser/Rust dynamic-app smoke proves real-context collection/property/cached/raw denial, cached read after explicit grant, plus a persisted runtime-installed `openModule(mod)` mount after reload with collection/property/cached/raw denial. Phase 13D static drift guard is implemented and hardened for optional chaining, dynamic property access and local DB aliases. Phase 13E is closed: runtime-installed/generated apps now expose an explicit same-origin trusted-code capability contract through `ctx.runtimeCapabilities`, the installed-app validator rejects forbidden network fetch, dynamic import, browser storage, Shell global state, cached `ctx.db`, Worker/navigation/eval and direct CTOX control-command bypasses, and Browser/Rust dynamic-app smoke proves the real `openModule` fixture sees the runtime-safety contract after reload. Phase 13F is closed: Shell/App Store UI preference storage is workspace/actor scoped where relevant through `business-os-storage-scope-v1`, and Dynamic Apps, Audience and Release Browser/Rust smokes prove browser storage remains non-authoritative for app visibility, release state, audience state and data grants. Phase 13C packaged/starter user-module batch is implemented for `coding-agents`, `calendar`, `buchhaltung`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `matching`, `notes`, `outbound`, `research`, `shiftflow`, `spreadsheets` and `support`: the guarded facade blocks collection/property/raw/context access before `data.read`, exposes matching `ctx.permissions` decisions, renders a real Support Shell locked state without data grants, allows read after an exact collection grant and keeps writes denied without `data.write`; `coding-agents`, `calendar`, `buchhaltung`, `customers`, `invoices`, `iot`, `matching`, `notes`, `outbound`, `research` and `shiftflow` are smoke-tested against module-owned collections after targeted schema registration. `customers` additionally degrades optional linked cross-app projections to an empty linked-data state on permission denial instead of blocking core CRM load, `calendar` no longer unwraps `ctx.db.raw` and gates default seed writes behind collection write permission, `buchhaltung` no longer unwraps `ctx.db.raw`, no longer exports module state globally, reconciles `accounting_number_series` in manifest/schema/fallback metadata, gates automatic chart/demo writes behind exact accounting collection write permission and removes UI-E2E localStorage asset data fallback, `invoices` no longer exposes module `STATE`/`ctx.db` through its debug bridge, `iot` no longer prefers `ctx.db.raw` in its collection resolver, `matching` no longer injects `ctx.db.raw` or opens its own CTOX DB fallback and resolves/writes `matching_requirements`, `matching_objects` and `matching_results` through `ctx.db.collection(name)` plus `ctx.permissions`, `notes` no longer unwraps `ctx.db.raw`/`notes_records` or uses LocalStorage as an authoritative note-data fallback, `outbound` now resolves Active Outreach through the guarded facade, gates automatic default/import-repair writes behind collection write permission and treats `ctox_queue_tasks` as optional read-permission-aware operational status outside the Outbound module grant, `research` now resolves reads through `ctx.db.collection(name)`, gates task/run writes, treats command/queue/Fachbericht document access as optional/read-permission-aware, and declares the document collections in manifest/schema, and `shiftflow` removes global/DOM cached DB handles, gates startup seed writes behind collection write permission and mounts subscriptions through guarded helpers. Phase 13 system raw cleanup has additionally removed direct raw DB access from Browser, CTOX, Knowledge, Reports and Tickets; those system modules now resolve collections through `ctx.db.collection(name)`. Creator runtime and generated-app template also no longer use collection-property or `ctx.db.collections` fallback access. Phase 13G removes the remaining guarded-module collection-property/proxy fallbacks from Buchhaltung, Calendar, Coding Agents, Customers, CV Print Builder, Documents, Invoices, IoT, Notes, Outbound, Shiftflow, Spreadsheets and Support; all 24 module inventory entries now report raw/property/proxy/cached-handle flags as false. Phase 13H replaces the last unscoped Shell/Desktop facades with explicit allowlists for Settings, Business Chat, Business Reporter and each Desktop app; the inventory guard now reports 24 modules, 5 desktop apps and 0 unscoped facades. Inventory/conformance guards pass, Dynamic Apps Browser/Rust smoke proves all 16 packaged guard modules warning-clean, `business-os-agent-scope-ui`, `business-os-roles-permissions-ui` and `workspace-large-file-viewer-rust-to-browser` pass clean for the new scoped facades, and the UI-regression Browser/Rust smoke exercises the broader Shell paths. Phase 13 scoped system/internal exceptions are closed: App Store, Browser, Creator, CTOX, Desktop, Knowledge, Reports and Tickets are served by `SCOPED_SYSTEM_MODULE_DB_COLLECTIONS`, the inventory stores exact `scoped_collections`, and the Dynamic Apps Browser/Rust smoke proves 8/8 scoped modules with allowed collection access, foreign collection/property/raw denial, permission-facade parity and capability contract evidence. Phase 13 is complete locally |
| 14 | Production browser E2E auth, tenant and reload matrix | Complete | Codex | Phase 14A-14F are implemented and passing for the current local-workspace production claim: release, audience, agent-scope, auth-scope and fresh-profile modes are registered in runner/matrix with required evidence contracts, self-test/negative checks and count/budget maximums where needed. Auth-scope proves login, authenticated reload, logout, logged-out reload, protected-access block, stable local tenant/workspace scope, clean browser context, forged stored pairing/auth data not widening scope and `local-workspace-only` as the explicit tenant claim. Fresh-profile proves clean profile startup, authoritative projection, lifecycle/version labels, disabled reasons, desktop/narrow viewport states, browser-storage non-authority and a representative scale fixture with 57 catalog apps, 64 explicit grants, 96 release versions, 128 native audit events and App Store/render/start-menu budgets. Browser warnings/errors/404/request failures are 0 for the accepted auth and scale fresh-profile runs. Hosted/multi-workspace isolation is not claimed by this local release and stays future hosted-product scope |
| 15 | Observability, audit retention and support recovery | In progress | Codex | 15A Shell/browser, native command and Settings live Browser/Rust slices are implemented: the lifecycle drawer renders business-facing "Warum?" diagnostics, `ctox.business_os.why` returns native actor/app/action/data decision diagnostics with sanitized command projection tests, and Settings module management dispatches/renders those diagnostics with Browser/Rust evidence for visible rows and redaction. The support-safe diagnostics artifact path is implemented end-to-end: `ctox.business_os.support.export_diagnostics` returns `ctox.business_os.support_diagnostics.v1`, Settings exposes a per-app Support-Paket action with business-facing schema/protection/scope/activity/why summary plus JSON download, and Browser/Rust requires visible rows, redaction and download evidence. MCP audit retention is typed native policy state via `business_os.mcp_policy.v1`; legacy `CTOX_BUSINESS_OS_MCP_*` runtime-env values are only a migration fallback until a typed policy exists. Native `ctox.business_os.audit.retention` exports expired `business_events` to a support-safe artifact before optional prune, and native `ctox.business_os.audit.retention_policy.set` persists validated retention days as `business_os.audit_retention_policy.v1` for requests that omit `retention_days`. Native `ctox.module.repair_lifecycle_projection` supports dry-run release/catalog recovery plus optional stale module-scoped grant repair through `repair_stale_grants`, invalid source/rollback version-ref repair through `repair_invalid_version_refs` and orphan private-app responsibility recovery through `repair_orphan_private_apps`, with sanitized command projection and tests for dry-run/apply. `ctox.module.rollback_version` has explicit manifest/source-file restore evidence for `module.json`, editable source files and post-target added-file removal. Native backup/restore drill coverage exists as CLI-only `ctox business-os backup restore-drill`, including online SQLite snapshots for core state, CTOX Secret Store, Business OS store and native RxDB store, installed modules, source snapshots, audit exports, isolated restore validation and support-safe preflight for typed MCP and native audit-retention policy. The drill manifest now includes Secret-Store-backed HMAC-SHA256 integrity, local raw-backup retention/support-attachment policy, same-version/downgrade compatibility policy and a chunked AES-256-GCM portable snapshot export whose decrypt/ZIP verification is asserted before handoff; `ctox business-os backup prune-drills` deletes only expired drill directories with manifest retention metadata. The drill and preflight include a machine-readable active-root restore runbook with quiesce, manifest signature/hash verification, portable-export/key-escrow gates, restore-target and restart gates, without overwriting the active root. Browser/Rust `business-os-restore-resync-ui` now proves a same-profile browser IndexedDB write made while the native peer is stopped remains local, then converges to native SQLite over WebRTC after peer restart with warnings/errors/request failures 0. Phase 15 remains open for hosted/multi-workspace WebRTC restore proof, release-level cross-version/downgrade restore evidence and external key-escrow operational signoff |
| 16 | Release packaging, CI gates and rollout runbook | In progress | Codex | Static required-smoke-mode CI guard is implemented via the existing Business OS RxDB contract job, the smoke matrix now writes a fixed-path schema-validated summary artifact, Business OS JS/module/browser-test dependencies are declared and CI-bootstrapped from `src/apps/business-os/package-lock.json`, and CI/tag-release workflows now run/upload the warning-clean Business OS production Browser/Rust gate before release artifacts. Legacy fixtures are tested, customer/operator docs have dry-run evidence, a machine-checked release-signoff artifact exists, and release tags block while the signoff is pending. Full phase remains open for actual security/privacy signoff and final customer/operator release review before full production-ready claim |

Allowed status values: `Not started`, `In progress`, `Blocked`,
`Ready for review`, `Complete`.

## Full Production-Ready Definition

The Business OS roles/permissions and dynamic-app lifecycle work is fully
production ready only when all items below are true. Backend tests, Rust
policy tests and schema checks are required but not sufficient.

Hard requirements:

1. Real browser user stories pass in a clean profile for Owner, Admin,
   App-Verantwortliche:r and Teammitglied.
2. Auth flows are tested where auth is available: login, authenticated reload,
   logout, logged-out reload and blocked protected access after logout.
3. Tenant/workspace scoping is visible in the UI: no user sees apps, data,
   source, release controls or app-governance actions outside their scope.
4. Dynamic app lifecycle is persisted and projected through the existing
   RxDB/WebRTC data path, not through HTTP and not through client-only state.
5. App Store release flow can move an app from private `0.x` to Team `1.0.0+`
   only after SemVer, source snapshot, release checks, data-access review and
   rollback evidence are complete.
6. Preview and restricted audiences are editable through business-facing UI and
   enforce app visibility plus data access consistently after reload.
7. Human and AI/agent actors use the same permission model; an agent never gets
   invisible extra app or data access.
8. Runtime-installed apps and migrated packaged/core modules cannot bypass
   `data.read`/`data.write` through `ctx.db`, collection properties, cached
   handles or `ctx.db.raw`.
9. Console errors, failed requests, WebRTC/RxDB sync failures, stuck loaders and
   unexpected browser warnings are hard release blockers unless explicitly
   classified and budgeted in the smoke matrix.
10. Audit and support evidence exists for policy denials, releases, rollbacks,
    visibility changes, data-access changes, agent delegations and external
    approvals.
11. CI/release gates run the required Rust, JS, RxDB and Browser/Rust smoke
    matrix without known skips for this surface.
12. Operator runbooks document rollout, rollback, diagnosis, recovery and
    customer-facing support boundaries.
13. Dynamic app runtime behavior has an explicit trusted-code boundary:
    generated/installed apps cannot gain data, network, storage or external
    action capabilities outside the product contract merely because their JS
    runs in the Shell context.
14. Browser storage is scoped and non-authoritative for permissions:
    `localStorage`, `sessionStorage` and IndexedDB startup state may remember
    layout/session hints, but cannot decide visibility, grants, release state
    or tenant/workspace authority.
15. Concurrency and offline/reconnect behavior are tested for release,
    rollback, audience and grant changes: duplicate commands, stale catalog
    rows and out-of-order sync cannot produce a user-visible false success.
16. Performance and scale budgets exist for representative teams, app counts,
    grant counts, audit volume and release history; slow paths produce visible
    loading/progress states rather than stuck shells.
17. Security/privacy signoff is complete for the changed surfaces, including
    redaction, prompt/context boundaries, app-source visibility, agent
    delegation, external effects and release artifact integrity.

Minimum production user stories:

1. Owner invites/administers users, assigns App-Verantwortliche, publishes a
   private app, restricts it again, rolls it back and can explain every state
   from Activity/diagnostics.
2. Admin manages Team and released apps but cannot transfer Owner role, cannot
   accidentally expose private `0.x` apps and cannot bypass release/data-review
   gates through old Settings or HTTP paths.
3. App-Verantwortliche:r creates or receives a private `0.x` app, sees it in
   Shell/App Store, edits source, reviews data areas, publishes `1.0.0+`,
   grants preview audience, rolls back with `apps.rollback` and sees failed
   validations as understandable command failures.
4. Teammitglied sees only Team-visible or explicitly previewed apps, cannot
   right-click/change/source-view/release/rollback apps without grants, and sees
   locked data areas instead of raw errors when data grants are missing.
5. Agent/service actor can see/open only app-visible modules, gets data only
   through explicit `data.read`/`data.write`, shows actor/app/data/external
   scope before action, and creates audit evidence for allowed and denied
   delegations.
6. Operator/support can export a redacted diagnostic bundle that explains
   app visibility, release state, data grants, audit history and recovery
   options without leaking secrets, prompt bodies or raw record payloads.

Gate types required for every production-affecting phase:

- Backend gate: Rust/native policy, persistence, projection, migration and
  failure-path tests.
- Browser gate: real Browser/Rust story with role, auth/profile state,
  reload/fresh-profile facts, visible UI labels and console/network budgets.
- Source guard: static or conformance check preventing forbidden patterns such
  as HTTP data paths, stale Settings publish controls or unscoped DB access.
- Migration gate: legacy fixture/backfill/downgrade checks for existing
  installs.
- Operations gate: audit/diagnostic/recovery evidence and runbook updates.
- Release gate: CI or release workflow proof that the phase cannot be skipped
  before shipping artifacts.

Production completion checklist:

| Element | Phase owner | Required production evidence |
| --- | --- | --- |
| Role model and business labels | 0-7, 14 | Owner, Admin, App-Verantwortliche:r and Teammitglied pass backend policy, Shell/App Store/Settings affordance tests and Browser/Rust role stories without raw role labels leaking into user-facing UI |
| App version default visibility | 8-11 | Runtime-installed `0.x` apps stay private to responsible builders; `1.0.0+` apps become Team-visible only through projected lifecycle state; invalid/missing SemVer stays private after reload and fresh profile |
| App icon/version/badge semantics | 8, 10, 11, 14 | Shell tabs, launcher/start-menu, Appbar and App Store show version plus privacy/release/audience badges; badge click opens the right governance surface; desktop and narrow viewport checks prove full German labels remain readable or accessibly expanded |
| Right-click and source controls | 3-4, 10, 14 | Context menu, Appbar and Source Editor expose modify/source/release/rollback only to actors with projected rights; denied states show business reasons where the action is high-risk |
| App Store publish/review workflow | 10 | Publish wizard covers target version, source snapshot, release notes, data review, responsible users, rollback target and final summary; success renders only after native catalog projection confirms the release |
| Release/rollback consistency | 10, 15 | 10D1 manifest/SQLite failure cases pass; 10D2 now passes duplicate release replay and lifecycle projection repair for release records plus catalog; Phase 15 supplies broader operator recovery, support diagnostics and runbooks for divergence |
| Data-access review and data grants | 10, 12, 13, 14 | Review collections match manifest read/write contract, Team release requires explicit data grants or locked-state behavior, UI distinguishes granted vs locked areas and real app runtime DB access cannot bypass `data.read`/`data.write` |
| Audience and preview management | 11, 14 | Private, Preview, Team and Eingeschraenkt are durable native/RxDB state separate from edit/source/release/data grants; deep links, reload and fresh-profile browser stories preserve audience correctly |
| Responsible builder ownership | 11, 15 | Creator/App-Verantwortliche state cannot become orphaned on user removal; reassignment or Owner/Admin recovery is audited and tested |
| Agent/app-scope parity | 12, 14, 15 | MCP and in-app AI see app visibility before data grants, show active actor/app/data/external scopes before action, reject spoofed client context and audit allowed/denied delegations |
| Packaged/core module DB isolation | 13, 14 | Committed inventory classifies every packaged/core module; user-facing modules use the guarded DB facade; system exceptions are owner-assigned, tested and documented |
| Dynamic app runtime safety | 13, 14, 16 | Runtime-installed app JS has an explicit trusted-code/security boundary, external fetch/import/storage behavior is classified, and forbidden runtime capabilities are guarded by static checks plus Browser/Rust negative app fixtures |
| Browser storage boundary | 13, 14 | UI preference storage is scoped by workspace/user where relevant and never acts as the authority for permissions, release state, audience state or tenant scope; fresh-profile and cross-profile smokes prove the same projected state |
| Auth, tenant and profile boundaries | 14 | Login, authenticated reload, logout, logged-out protected access, clean profile, fresh profile and tenant/workspace scoping pass without process-env production toggles |
| Performance and scale budgets | 14, 16 | Production smokes or load fixtures cover representative app catalog size, grants, audit rows and release history; startup/sync/release actions have budgets and user-visible progress/error states |
| Observability and support diagnostics | 15 | "Warum?" diagnostics explain app visibility, source/edit/release, data grants, audience and agent decisions in Shell and Settings; redacted support export exists before any prune/retention action |
| Backup and restore | 15, 16 | Runtime store, module manifests/source snapshots, release rows, audit data and recovery commands have tested backup/restore or documented manual boundaries before destructive rollout steps |
| Migration and rollback runbook | 15, 16 | Legacy fixtures cover private `0.x`, invalid/missing SemVer, released `1.x`, restricted and preview apps; downgrade/repair steps are documented and tested where state changes are persisted |
| Security/privacy release signoff | 16 | Threat model/security checklist is updated for dynamic app code, app source visibility, data-review UX, MCP/agent scopes, audit exports, external effects and release artifact integrity |
| CI and release gates | 16 | Clean checkout bootstraps JS/Rust dependencies, runs fixed Browser/Rust smoke artifacts with schema validation, blocks release uploads unless required Business OS gates pass and records skipped tests explicitly |

Source-validated production blockers that must be closed before the label
`Full product production-ready` can be used. The Production Blocker Ledger is
the authoritative status view; closed rows remain listed for traceability:

- Closed in Phase 10A: `src/core/business_os/store.rs::rollback_module_release`
  and the source rollback path enforce `apps.rollback` end to end.
- Closed in Phase 10A: `ctox.module.rollback` and
  `ctox.module.rollback_version` persist failed `business_commands` outcomes
  for missing versions or failed restore operations.
- Closed in Phase 10A: `ctox.module.list_versions` authenticates the actor and
  requires module-scoped `apps.source.view`, `apps.release` or
  `apps.rollback` before returning version timeline metadata.
- Closed in Phase 10D1/10D2/10E2 for backend consistency: release/rollback now
  guard stale source and rollback version refs before writing `module.json`,
  synchronise release rows and manual source-version rows in one SQLite
  transaction, restore `module.json` on injected release/rollback DB failures,
  resolve runtime-installed manifest paths consistently, project current
  release/rollback/data-area summary into lifecycle governance, avoid duplicate
  release side effects on command replay and provide a tested lifecycle
  projection repair command. Broader operator recovery and support drills remain
  Phase 15 blockers, not `P10-BE-CONSISTENCY`.
- Closed in Phase 10A/10B: Release data-access review validates reviewed
  read/write collections as manifest subsets, persists evidence-only semantics
  with no implied grants, and reconciles Team read/write collections with
  explicit `data.read`/`data.write` grants or a declared locked-state
  behavior.
- Runtime-installed app DB isolation must be proven in the real module context:
  `openModule(mod)` must pass the active module into the guarded DB facade so
  `ctx.db`, collection handles and `ctx.db.raw` cannot bypass
  `data.read`/`data.write`.
- Full runtime app DB isolation is open until the real Shell mount path, not
  only smoke hooks, proves `ctx.db.collection`, `ctx.db.<collection>`, cached
  handles and `ctx.db.raw` are guarded after reload.
- Phase 11A introduces `apps.view` as the non-public app visibility grant:
  native projection and browser lifecycle checks no longer treat `apps.modify`,
  `apps.source.view`, `apps.release` or `apps.rollback` as routine visibility
  grants. Durable named audience state, migration from lifecycle hints and
  deep-link/fresh-profile proof remain separate Phase 11 blockers.
- MCP app visibility must be evaluated separately from `data.read` and
  `data.write`; a data grant unlocks data only after the actor can see/open the
  app lifecycle audience.
- App Store release flows must dispatch `business_commands` over RxDB/WebRTC.
  Settings release/rollback remains read-only diagnostics unless it is later
  upgraded to the same command payload and gates. Do not re-add
  `/api/business-os/modules/source`, `/modules/release` or `/modules/rollback`
  to the HTTP control-plane allowlist.
- Browser/Rust smoke modes must exist for release, audience, agent scope,
  auth/scope, fresh profile and diagnostics. A matrix row is not evidence until
  the mode exists and records the visible UI facts it claims.
- Release smoke must write a machine-readable summary to a fixed path and CI
  must fail when required modes, evidence keys or warning-budget fields are
  missing, even if the browser process exits successfully.
- The tag/release workflow must run the Business OS production gates before
  uploading release artifacts.
- Auth and tenant production checks must be configured through typed runtime
  config or test-root persisted state. No new process-env production toggles are
  allowed.
- Native `business_events` now have a redacted export-before-prune command.
  Native audit retention days are persisted as typed Business OS state; remaining
  native audit work is volume/operations policy where required plus recovery
  drills. Native isolated backup/restore drill coverage exists; active-root
  restore has a machine-readable manual runbook/preflight, and local same-profile
  browser unsynced-state handling is Browser/Rust-proven through WebRTC peer
  restart. Raw backup manifests are signed through the CTOX Secret Store and
  carry local retention/support-attachment policy. Hosted/multi-workspace WebRTC
  restore and release-level cross-version restore evidence remain open. Portable
  raw backup encryption is implemented through an AES-GCM export, and
  `ctox business-os backup key-escrow-status` exposes redacted key-fingerprint
  evidence for the external escrow record while leaving final escrow approval as
  an operator gate. MCP audit retention now uses typed
  `business_os.mcp_policy.v1` policy state; legacy runtime-env policy remains a
  migration fallback only.
- Packaged/core module isolation now has a committed inventory artifact with
  every module classified, owner assigned and exception tested; Phase 13 is
  complete locally.
- Phase 13 system raw cleanup removed direct `ctx.db.raw` access from Browser,
  CTOX, Knowledge, Reports and Tickets, removed Creator property/proxy DB
  fallback access, removed remaining guarded-module property/proxy access and
  scoped the formerly naked Shell/Desktop facades, and now serves remaining
  system/internal exceptions through exact tested scoped collection allowlists.
- Existing-install migration readiness requires committed legacy fixtures for
  private `0.x`, missing version, invalid SemVer, released `1.x`, restricted
  and preview apps.
- Clean-checkout release gates must document and CI-enforce the supported Node
  dependency bootstrap for Business OS JS tests. No gate may depend on local
  symlinks or undeclared host-global `esbuild`/Playwright installs.
- Runtime-installed app JS currently mounts in the Shell module context through
  dynamic import. Full production readiness needs an explicit trusted-code
  boundary, runtime capability inventory and negative fixtures for forbidden
  data/storage/network/external-effect bypasses.
- Browser `localStorage`/`sessionStorage` currently stores several Shell,
  App Store and Creator UI preferences. Production readiness requires proving
  those stores are scoped hints only and never the authority for role,
  visibility, release, audience, tenant or data grants.
- Release/audience/agent/auth/fresh-profile smoke modes are registered,
  implemented and guarded by registry self-tests plus fixed evidence contracts.
  The production gate now rejects missing evidence and successful attempts that
  exceed their recorded warning/startup/sync/duration budgets.
- Performance and scale budgets are attached to the Fresh Profile production
  smoke: it uses a representative app catalog, explicit grants, release
  history and native audit-event fixture, then fails on missing counts or
  exceeded UI render/start-menu/App-Store budgets.
- Security/privacy signoff for dynamic app code, source visibility, agent
  delegation, data-review UX, audit exports and external effects must be a
  release gate, not an after-the-fact review.
- Backup/restore has native isolated drill coverage for the core stores, CTOX
  Secret Store, app manifests, source snapshots, release rows and audit/export
  state, plus signed manifest/retention compatibility metadata and an encrypted
  portable AES-GCM export with local decrypt/ZIP verification. Downgrade,
  active-root restore and hosted browser/WebRTC state must still be tested or
  explicitly documented as manual operator work before destructive rollout; the
  portable encryption key must be escrowed outside the encrypted artifact.

## Plan Operating Model

This document is not a static concept note. During implementation, every phase
must be treated as a small release candidate with its own source anchors,
changed files, tests, browser evidence, migration notes and residual risks.

Update rules:

1. Before code changes start for a phase, mark the phase `In progress` and add
   the current source anchors that justify the planned implementation.
2. When implementation changes the source shape, update the phase scope in this
   file in the same working round. Do not keep obsolete command names, UI paths
   or test modes in the plan.
3. A phase can move to `Ready for review` only after all required local tests
   and browser smokes listed for that phase have run or are explicitly recorded
   as blocked with reason, owner and next step.
4. A phase can move to `Complete` only when backend behavior, browser UX,
   persistence/reload behavior, audit evidence and rollout/rollback notes are
   all covered for that phase's blast radius.
5. Any production blocker discovered during implementation must be added to the
   blocker ledger below before the turn ends. If the blocker is fixed in the
   same round, keep the ledger row and mark it closed with evidence.
6. Do not mark a phase complete based only on unit tests when the phase changes
   visible app lifecycle, permissions, auth, tenant scope, release, rollback or
   agent behavior. Those surfaces require browser evidence.
7. Keep source validation concrete: reference files/functions or smoke modes,
   not general subsystem names.

Per-phase completion template:

| Field | Required content |
| --- | --- |
| Source anchors | Exact files/functions/UI modules reviewed before and after implementation |
| Backend changes | Native commands, policy decisions, persistence, migration/backfill and audit behavior |
| UI/UX changes | Shell/App Store/Settings/right-click/app-mode states, labels, disabled reasons, loading/error/empty states |
| Data and sync | RxDB/WebRTC projection, reload/fresh profile behavior, no HTTP fallback, no dist patch |
| Tests run | Exact commands, result, date, log/artifact path and any skipped tests |
| Browser evidence | Smoke mode, URL, role, auth state, tenant/workspace, visible facts and console/network status |
| Migration/rollback | Existing-install behavior, downgrade notes, repair/recovery path |
| Residual risk | Explicit open risk, owner, expiry or follow-up phase |

## Production Blocker Ledger

The rows below are source-validated blockers, not speculative improvements.
They must stay visible until closed with evidence.

| ID | Phase | Blocker | Source anchor | Required closure evidence | Status |
| --- | --- | --- | --- | --- | --- |
| P10-BE-ROLLBACK-PERM | 10 | Rollback helpers must enforce `apps.rollback` internally, not `apps.modify`. Dispatcher checks alone are insufficient for HTTP/control callers and future reuse. | `src/core/business_os/store.rs:3743`, `src/core/business_os/store.rs:5456`, `src/core/business_os/store.rs:21479`, `src/core/business_os/store.rs:21686` | PASS 2026-06-17: `module_release_rollback_command_uses_apps_rollback_without_modify_permission` and `module_source_rollback_version_uses_apps_rollback_without_modify_permission` in `cargo test --bin ctox module_ -- --nocapture` | Closed |
| P10-BE-FAILED-OUTCOME | 10 | `ctox.module.rollback` and `ctox.module.rollback_version` must persist failed command outcomes when restore/version lookup fails. | `src/core/business_os/store.rs:6112`, `src/core/business_os/store.rs:9203`, `src/core/business_os/store.rs:9257`, `src/core/business_os/store.rs:21571` | PASS 2026-06-17: `module_rollback_commands_persist_failed_outcomes` in `cargo test --bin ctox module_ -- --nocapture` | Closed |
| P10-BE-VERSION-LIST-GATE | 10 | `ctox.module.list_versions` must authenticate and require module-scoped source/release/rollback rights before returning source/release timeline metadata. | `src/core/business_os/store.rs:6146`, `src/core/business_os/store.rs:9226`, `src/core/business_os/store.rs:21751` | PASS 2026-06-17: `module_list_versions_requires_source_release_or_rollback_rights` in `cargo test --bin ctox module_ -- --nocapture` | Closed |
| P10-BE-DATA-REVIEW-EVIDENCE | 10 | Release data-access review must reject reviewed read/write collections outside the manifest and must explicitly persist that review evidence creates no data grants. | `src/core/business_os/store.rs:1779`, `src/core/business_os/store.rs:21836`, `src/core/business_os/store.rs:21903` | PASS 2026-06-17: `module_release_rejects_data_review_access_outside_manifest` and `module_release_stores_data_review_as_evidence_without_implied_grants` in `cargo test --bin ctox module_ -- --nocapture` | Closed |
| P10-BE-DATA-REVIEW-GRANTS | 10 | Release data-access review must reconcile reviewed read/write collections with explicit `data.read`/`data.write` grants or a declared locked-state behavior before a Team release is considered production-safe. Evidence-only validation is not enough. | `src/core/business_os/store.rs:1779`, `src/core/business_os/store.rs:1899`, `src/core/business_os/store.rs:1942`, `src/core/business_os/store.rs:22032`, `src/core/business_os/store.rs:22154`, `src/core/business_os/store.rs:22219` | PASS 2026-06-17: `module_release_stores_data_review_as_evidence_without_implied_grants`, `module_release_rejects_review_without_data_grant_or_locked_state`, `module_release_accepts_locked_review_without_implied_data_grants` in `cargo test --bin ctox module_ -- --nocapture`; UI rendering of granted vs locked data areas remains covered by `P10-UI-WIZARD` | Closed |
| P10-BE-CONSISTENCY | 10 | Release and rollback need an explicit consistency strategy across manifest file, release row, source snapshots and catalog projection. | `src/core/business_os/store.rs::record_module_release`, `src/core/business_os/store.rs::rollback_module_release`, `src/core/business_os/store.rs::sync_module_release_records`, `src/core/business_os/store.rs::repair_module_lifecycle_projections`, `src/core/business_os/store.rs::accept_rxdb_business_command`, `src/core/business_os/rxdb_peer.rs::accept_pending_business_command`, `src/core/business_os/store.rs::module_release_command_replay_does_not_duplicate_release_state`, `src/core/business_os/store.rs::module_lifecycle_projection_repair_resyncs_releases_and_catalog` | PASS 2026-06-17: stale `source_version_id`/`rollback_version_id` refs are rejected before manifest write; release row plus manual source-version snapshot sync in one SQLite transaction; release and release rollback restore `module.json` on injected DB failures; runtime-installed release snapshots resolve through the installed manifest root; lifecycle projection includes current release and rollback target; replaying the same `ctox.module.release` command returns the stored accepted outcome without adding a second release row, second manual source-version row or duplicate release audit event; `ctox.module.repair_lifecycle_projection` is `runtime.manage` gated and resyncs canonical `business_records`, the RxDB `business_module_releases` projection and `business_module_catalog`; the RxDB peer treats the repair command as a catalog-sync command. Evidence: `rustfmt --edition 2021 src/core/business_os/store.rs src/core/business_os/rxdb_peer.rs --check`; `cargo test --bin ctox module_release_command_replay_does_not_duplicate_release_state --target-dir runtime/build/core-rxdb-integration-target -- --nocapture` - 1 passed; `cargo test --bin ctox module_lifecycle_projection_repair_resyncs_releases_and_catalog --target-dir runtime/build/core-rxdb-integration-target -- --nocapture` - 1 passed; `cargo test --bin ctox module_release_ --target-dir runtime/build/core-rxdb-integration-target -- --nocapture` - 12 passed; `cargo test --bin ctox module_ --target-dir runtime/build/core-rxdb-integration-target -- --nocapture` - 42 passed; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `cargo check --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`. This backend consistency row did not close Settings Activity/browser audit by itself; `P10-BE-AUDIT` is closed separately by the release smoke evidence below. | Closed |
| P10-BE-NO-HTTP-REENABLE | 10 | App Store and Settings release flows must stay on RxDB/WebRTC `business_commands`; legacy HTTP source/release/rollback handlers must not be re-added to the control-plane allowlist. | `src/core/business_os/server.rs::handle_request`, `src/core/business_os/server.rs::is_business_os_control_plane_path`, `src/apps/business-os/scripts/assert-rxdb-only.mjs::assertBusinessOsServerHttpDataApisAreGated`, `src/apps/business-os/shared/command-bus.js` | PASS 2026-06-17: `server.rs` has no legacy module source/release/rollback route arms, request DTO or direct server helper/store markers; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `node --check src/apps/business-os/scripts/assert-rxdb-only.mjs`; `cargo check --bin ctox`; `cargo test --bin ctox module_ -- --nocapture` - 33 passed; `cargo test --manifest-path src/core/rxdb/Cargo.toml` - 239 unit and 30 conformance tests passed; App Store/Settings release UI dispatch remains covered by later Phase 10F/10G/10H browser gates | Closed |
| P10-BE-RELEASE-PROJECTION | 10 | Release review, release status, rollback target, data review state and locked/granted data-area summary must project into `business_module_catalog.governance.lifecycle` so Shell/App Store/Settings render confirmed native state after reload. | `src/core/business_os/store.rs::projected_module_lifecycle`, `src/core/business_os/store.rs::module_release_lifecycle_summary`, `src/core/business_os/store.rs::release_review_data_access_projection`, `src/core/business_os/store.rs::module_catalog_for_rxdb`, `src/core/business_os/store.rs::sync_module_release_records`, `src/apps/business-os/shared/app-lifecycle.js::appReleaseProjection`, `src/apps/business-os/modules/app-store/index.js::releaseProjectionBadgeHtml`, `src/apps/business-os/modules/app-store/index.js::releaseFactLinesForItem`, `src/apps/business-os/app.js::openAppLifecycleDrawer`, `src/apps/business-os/shared/react-settings.js::moduleReleaseProjectionHtml`, `src/apps/business-os/shared/react-settings.js::moduleReleaseDiagnosticsHtml` | PASS 2026-06-17: native lifecycle projection exposes `release_status`, `release_state.current`, `release_state.history_count`, `rollback_target` and `data_access` with granted/locked collection ids; UI/static consumers render release/rollback/data-area summaries in App Store, Shell lifecycle drawer and Settings module rows. Browser/Rust `business-os-app-release-ui` proves the App Store waits for native catalog projection before success: after publishing the runtime-installed `phase10-release-app` from private `0.8.0` to Team `1.0.0`, it requires Team visibility, projected `v1.0.0`/`Team` badge, data-review details, reload evidence, storage-boundary check and rollback completion. Evidence: `node src/apps/business-os/shared/app-lifecycle.test.mjs` - 11 passed; `node src/apps/business-os/shared/react-settings.test.mjs` - 6 passed; `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 18 passed; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-release-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60075 SIGNALING_PORT=60076 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with browser errors/404/request failures 0. | Closed |
| P10-BE-AUDIT | 10 | Release, rollback, failed release validation and failed rollback outcomes need queryable business audit evidence, not only command result blobs. | `src/core/business_os/store.rs::insert_business_event`, `src/core/business_os/store.rs::module_lifecycle_audit_event_type`, `src/core/business_os/store.rs::module_lifecycle_audit_summary`, `src/core/business_os/store.rs::record_business_module_lifecycle_event`, `src/core/business_os/store.rs::accept_rxdb_business_command`, `src/core/business_os/store.rs::module_release_and_rollback_write_business_event_audit`, `src/core/business_os/store.rs::module_release_failed_validation_writes_business_event_audit`, `src/core/business_os/store.rs::module_release_rollback_failed_outcome_writes_business_event_audit`, `src/apps/business-os/shared/react-settings.js::activityTitle`, `src/apps/business-os/shared/react-settings.js::activityDetail`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsAppReleaseUiSmoke`, `src/core/rxdb/tools/business_os_production_smoke_registry.js` | PASS 2026-06-17: native release/rollback success, `ctox.module.rollback_version`, failed release validation and failed rollback outcomes write `business_events` with business-facing summaries, and `ctox.business_os.audit.list` returns those event types. Settings Activity renders `App-Version veröffentlicht` and `App-Rollback angewendet` without raw event names, `data_access_review`, locked collection IDs or raw command payload text. Browser/Rust `business-os-app-release-ui` opens the real Settings drawer after App Store release/rollback and requires `business_os_app_release_release_audit_visible=1`, `business_os_app_release_rollback_audit_visible=1` and `business_os_app_release_activity_audit_redacted=1`. Evidence: `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_source_rollback_version_uses_apps_rollback_without_modify_permission -- --nocapture` - 1 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_rollback_commands_persist_failed_outcomes -- --nocapture` - 1 passed; `node src/apps/business-os/shared/react-settings.test.mjs` - 6 passed; `node --check src/apps/business-os/shared/react-settings.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo build --bin ctox`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-release-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60075 SIGNALING_PORT=60076 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK, browser warnings/errors/404/request failures 0. | Closed |
| P10-UI-WIZARD | 10 | App Store still needs a business-facing publish wizard and rollback UX using release/data-review payloads. | `src/apps/business-os/modules/app-store/index.js`, `src/apps/business-os/modules/app-store/index.css`, `src/apps/business-os/modules/app-store/app-store.test.mjs`, `src/apps/business-os/shared/react-settings.js` | PASS 2026-06-17: App Store cards expose a `Freigeben` action only for runtime-installed apps with `apps.release`; the release dialog builds a `ctox.module.release` payload with target SemVer, source snapshot, rollback target, responsible users, release notes and evidence-only `data_access_review` using `read_collections`, `write_collections`, `locked_read_collections`, `locked_write_collections`, `locked_state_behavior`, `review_is_evidence_only=true` and `grants_implied=false`. Tests prove denied/granted `apps.release`, 0.x default target `1.0.0`, manifest-only collection filtering and no implied grants. Browser/Rust `business-os-app-release-ui` proves publish/reload/rollback through the real App Store UI and native `business_commands` path. Evidence: `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 18 passed; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-release-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60075 SIGNALING_PORT=60076 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with required evidence keys all true and browser errors/404/request failures 0. | Closed |
| P10-SETTINGS-FALLBACK | 10 | Settings release controls must either use the same Phase-10 release payload and gates as App Store or be downgraded to read-only/expert diagnostics to avoid stale publish paths. | `src/apps/business-os/shared/react-settings.js::moduleReleaseDiagnosticsHtml`, `src/apps/business-os/shared/react-settings.js::moduleReleaseProjectionHtml`, `src/apps/business-os/shared/react-settings.test.mjs`, `src/apps/business-os/app.js`, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js` | PASS 2026-06-17: Settings module rows render disabled read-only Freigabe/Rollback diagnostics, no active `data-module-release`, `data-module-rollback`, `data-rollback-version` controls, and no Settings `commandType: 'ctox.module.release'` / `ctox.module.rollback` dispatch path remain; `node src/apps/business-os/shared/react-settings.test.mjs` - 6 passed; `node --check src/apps/business-os/shared/react-settings.js`; `node --check src/apps/business-os/app.js`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`; Browser/Rust matrix `business-os-roles-permissions-ui` passed with `business_os_roles_permissions_settings_release_fallback_readonly=1`, `startup_smoke_hook_reload_count=0`, `startup_smoke_hook_wait_ms=79`, `browser_error_count=0`, `browser_resource_404_count=0`, `browser_request_failure_count=0`. | Closed |
| P11-AUDIENCE-GRANT | 11 | Preview/restricted audience state must not rely on `apps.modify` as visibility grant. | `src/core/business_os/store.rs::projected_module_lifecycle`, `src/apps/business-os/shared/app-lifecycle.js::appLifecycleState`, `src/apps/business-os/shared/app-lifecycle.js::canSeeModuleForAppVersion`, `src/core/business_os/policy.rs::BusinessOsPermission`, `src/core/business_os/policy.rs::evaluate`, `src/apps/business-os/shared/permissions.js::BusinessOsPermissions`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsDynamicAppsUiSmoke` | PASS 2026-06-17: `apps.view` added to native/browser permission models; `projected_module_lifecycle` derives preview grants only from active module-scoped `apps.view`, not `apps.modify`/source/release/rollback; browser lifecycle uses `apps.view` for non-public visibility while assigned App-Verantwortliche remain visible; JS tests prove Modify-only users cannot see private/restricted apps and `apps.view` does not imply edit/source/data; Rust catalog test proves Modify-only grants keep 0.x apps private; Browser/Rust dynamic-app smoke proves private hidden for Team, visible for App-Verantwortliche, Team `1.0.0` visible, restricted hidden for Team, data grants still separate and browser errors/404/request failures 0. | Closed |
| P11-AUDIENCE-PERSISTENCE | 11 | Preview/restricted audience must be durable native/RxDB state with migration from existing lifecycle hints, not browser-only `preview_user_ids` display inference. | `src/core/business_os/store.rs::backfill_manifest_preview_audience_grants`, `src/core/business_os/store.rs::projected_module_lifecycle`, `src/core/business_os/store.rs::module_catalog_for_rxdb`, `src/apps/business-os/shared/app-lifecycle.js::appLifecycleState`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsAppAudienceUiSmoke` | PASS 2026-06-17: `module_catalog_for_rxdb` idempotently backfills legacy runtime-installed `lifecycle.preview_user_ids` into active module-scoped `apps.view` grants, projected `preview_user_ids` are now derived from active `apps.view` grants rather than raw manifest display hints, JS lifecycle tests prove preview display still needs exact `apps.view`, and Browser/Rust `business-os-app-audience-ui` proves private hidden for Team, preview visible only for target, preview/restricted hidden for outside, reload/fresh-profile and storage-boundary evidence with browser warnings/errors/404/request failures 0. | Closed |
| P11-RESPONSIBILITY-ORPHAN | 11 | Private apps must not become orphaned when creator/App-Verantwortliche is removed; reassignment or explicit Owner/Admin recovery acceptance is required. | `src/core/business_os/store.rs::module_requires_active_responsibility`, `src/core/business_os/store.rs::assign_module_founder`, `src/core/business_os/store.rs::upsert_user`, `src/core/business_os/store.rs::module_lifecycle_projection_context`, `ctox.module.assign_founder`, `ctox.business_os.user.upsert`, `business_module_acl` | PASS 2026-06-17: direct Founder removal rejects deactivating the last active responsible user of a private runtime `0.x` app unless `accept_recovery_responsibility=true`; accepted recovery assigns the acting Owner/Admin as active App-Verantwortliche:r and audits both recovery assignment and removal; user deactivation follows the same rule and filters inactive users from lifecycle/permission projections; `ctox.module.assign_founder` and `ctox.business_os.user.upsert` persist failed orphan attempts as failed `business_commands` outcomes. Evidence: `rustfmt --edition 2021 src/core/business_os/store.rs src/core/business_os/rxdb_peer.rs --check`; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_founder_assignment -- --nocapture` - 3 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox user_deactivation_requires_recovery_for_sole_private_app_responsibility -- --nocapture` - 1 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_assign_founder_command_persists_failed_orphan_outcome -- --nocapture` - 1 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox user_upsert_command_persists_failed_private_app_responsibility_outcome -- --nocapture` - 1 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_catalog_projects_runtime_app_lifecycle_backfill -- --nocapture` - 1 passed. | Closed |
| P11-BADGE-DRAWER-PERMISSIONS | 11 | Lifecycle badge/drawer behavior must be permission-aware and show business data-area labels, not raw collection ids only; launcher/start-menu app-choice items need the same lifecycle/version signal as tabs and App Store cards. | `src/apps/business-os/app.js::buildStartMenuItem`, `src/apps/business-os/app.js::renderModuleTab`, `src/apps/business-os/app.js::renderModuleAppBar`, `src/apps/business-os/app.js::openAppLifecycleDrawer`, `src/apps/business-os/shared/shell-permissions-ui.js::buildLifecyclePermissionView`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsDynamicAppsUiSmoke` | PASS 2026-06-17: Shell start-menu app-choice items render `v0.1.0 Privat`, `v1.0.0 Team`, invalid private and restricted lifecycle badges for runtime-installed apps; badge clicks open the same lifecycle drawer without launching the app; drawer state is `manager` with `Verwalten erlaubt`, `App ändern` and App-Store management for App-Verantwortliche, and `readonly` with `Nur Ansicht`, no `App ändern`, and details-only App Store copy for Team actors. JS tests cover business-facing permission copy and app visibility vs management separation. Browser/Rust `business-os-dynamic-apps-ui` proves launcher badges plus manager/read-only drawer states through real DOM clicks with browser warnings/errors/404/request failures 0. Evidence: `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/shared/shell-permissions-ui.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs` - 5 passed; `node --test src/apps/business-os/shared/app-lifecycle.test.mjs` - 13 passed; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODE=business-os-dynamic-apps-ui SMOKE_PAGE_PATH=/index.html /Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node src/core/rxdb/tools/browser_rust_smoke.js` - pass with `business_os_dynamic_launcher_badges_visible=1`, `business_os_dynamic_lifecycle_drawer_manager_state=1`, `business_os_dynamic_lifecycle_drawer_readonly_state=1`, browser warning/error/404/request failure counts 0. | Closed |
| P11-DEEPLINK-LOCKED-STATE | 11 | Deep links to private/preview/restricted apps must produce consistent locked/read-only states for out-of-audience users and must not leak source/data/control UI before redirect. | `src/apps/business-os/app.js::openModule`, `src/apps/business-os/app.js::visibleModuleFallbackId`, lifecycle filters, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsAppAudienceUiSmoke` | PASS 2026-06-17: `openModule` now checks lifecycle visibility before desktop handoff, schema sync, module import or mount; hidden private/preview/restricted apps redirect to a visible fallback and surface a business-facing status reason. Browser/Rust `business-os-app-audience-ui` proves out-of-audience direct open is locked, in-audience preview remains visible after reload/fresh-profile storage reset, and localStorage taskbar/audience tampering cannot widen visibility. | Closed |
| P12-AGENT-SCOPE | 12 | Agent actions must show and enforce active app/data scope before AI-assisted action; no invisible extra access. | `src/core/business_os/mcp_channel.rs`, `src/apps/business-os/app.js::showGlobalCtoxContextMenu`, `src/apps/business-os/modules/app-store/index.js::buildAppStoreAgentScopeView`, `src/apps/business-os/shared/shell-permissions-ui.js::buildGlobalCtoxAgentScopeView`, `src/apps/business-os/shared/shell-permissions-ui.js::renderGlobalCtoxAgentScopeHtml`, `src/apps/business-os/shared/business-chat.js::renderChatAgentScopeHtml`, `src/apps/business-os/shared/react-settings.js::agentGrantBoundaryPanel`, `src/apps/business-os/modules/coding-agents/index.js::dispatchAgyCommand`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsAgentScopeUiSmoke` | PASS 2026-06-18: global right-click CTOX menu now renders business-facing `CTOX Zugriff` rows for actor, selected app/version/lifecycle, selected record, data summary and external-action state; the same prebuilt `agentScope` object is submitted as `client_context.visible_scope`; App Store context chat renders/builds selected-app actor/app/selection/data/external scope and denies direct app-modify detail creation for unassigned apps; Business Chat renders preserved `visible_scope` rows in the chat window when a chat was opened from scoped context; Settings renders active explicit app/data Sonderfreigaben as a read-only Owner/Admin boundary instead of exposing raw grant JSON or UI-only mutations; Coding Agents include provider, workspace root/path, session id, external target and surface in `client_context`. Browser/Rust `business-os-agent-scope-ui` opens a full-workspace runtime app, submits through the real global context menu and real App Store context menu, proves global and App Store `client_context` rows match the visible UI, proves Business Chat renders the submitted visible scope, proves Settings shows the read-only grant-boundary panel, proves hidden private app open is denied, proves `data.read` is denied before and allowed after an exact grant, proves write remains denied without `data.write`, and proves persisted command audit keeps visible scope. Evidence: `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/app-store/index.js`; `node --check src/apps/business-os/shared/react-settings.js`; `node --check src/apps/business-os/shared/shell-permissions-ui.js`; `node --check src/apps/business-os/shared/business-chat.js`; `node --check src/apps/business-os/modules/coding-agents/index.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 18 passed; `node src/apps/business-os/shared/react-settings.test.mjs` - 7 passed; `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs` - 7 passed; `node --test src/apps/business-os/shared/business-chat.test.mjs` - 3 passed; `node --test src/apps/business-os/shared/command-bus.test.mjs` - 2 passed; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; first Browser/Rust run passed all feature keys but failed startup budget (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=60135`); rerun passed. Final evidence: `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-agent-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60241 SIGNALING_PORT=60242 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_agent_scope_panel_visible=1`, `business_os_agent_scope_client_context_matches_ui=1`, `business_os_agent_scope_app_store_panel_visible=1`, `business_os_agent_scope_app_store_context_matches_ui=1`, `business_os_agent_scope_business_chat_scope_matches_context=1`, `business_os_agent_scope_settings_grant_boundary_visible=1`, `business_os_agent_scope_app_hidden_denied=1`, `business_os_agent_scope_data_denied_before_grant=1`, `business_os_agent_scope_read_allowed_after_grant=1`, `business_os_agent_scope_write_denied_without_grant=1`, `business_os_agent_scope_audit_visible=1`, browser warnings/errors/404/request failures 0. Native/MCP audit metadata parity is closed by `P12-AUDIT-METADATA`. | Closed |
| P12-MCP-APP-VISIBILITY | 12 | MCP app visibility must be evaluated separately from data access for `list_modules`, `get_module`, links, proposals and execution. | `src/core/business_os/mcp_channel.rs::list_modules`, `src/core/business_os/mcp_channel.rs::business_os_mcp_policy_decision`, `src/core/business_os/mcp_channel.rs::business_os_mcp_module_visibility_decision`, `src/core/business_os/mcp_channel.rs::module_value_visible_to_mcp_actor`, `src/core/business_os/policy.rs::BusinessOsPermission`, `src/core/business_os/policy.rs::evaluate` | PASS 2026-06-17: `list_modules` now filters raw native catalog modules by lifecycle public state or exact `apps.view`, not by `data.read`; `get_module`, `list_entities`, `list_module_actions` and `propose_action` first require app visibility and then `data.read`; module `open_link` requires app visibility without implying data access; `execute_action` first requires app visibility and then `data.write`. MCP tests prove `apps.view` makes a private preview app visible without `data.read`, `data.read`/`data.write` alone do not reveal or execute hidden private apps, and `1.0.0` apps are visible to Team by default. Evidence: `rustfmt --edition 2021 --check src/core/business_os/mcp_channel.rs`; `cargo test --bin ctox mcp_business_os_policy -- --nocapture` - 18 passed, 0 failed. | Closed |
| P12-CLIENT-CONTEXT-INTEGRITY | 12 | AI/right-click/App Store/Business Chat submitted `client_context` must match the visible selected app, actor and scope; it must never silently fall back to App Store/global module ids. | `src/apps/business-os/app.js::showGlobalCtoxContextMenu`, `src/apps/business-os/modules/app-store/index.js::appStoreContextChatDetail`, `src/apps/business-os/shared/command-bus.js::normalizeCommandClientContext`, `src/apps/business-os/shared/command-bus.test.mjs`, `src/apps/business-os/shared/business-chat.js::renderChatAgentScopeHtml`, `src/apps/business-os/shared/business-chat.js::initSchedulerLoop`, Shell context menu, App Store context chat, Business Chat, coding-agent handoff | PASS 2026-06-18: global right-click submits `module`, `module_id`, `app_id`, `source_module`, actor and `visible_scope`; App Store context chat submits selected app `module_id`/`app_id`, actor and `visible_scope` even for data-mode prompts and downgrades disallowed app-modify requests to data-mode; command bus normalizes caller-preserved `client_context` into canonical module/app ids, command action, mode, target, record, transport and `scope`, and does not overwrite caller-provided actor; `module_id`/`app_id`/`source_module` are considered before the old `ctox` fallback; Business Chat renders only existing `client_context.visible_scope`/`scope.visible_scope` and scheduled commands merge preserved `chat.contextMeta.client_context` before adding fresh chat metadata. Browser/Rust `business-os-agent-scope-ui` proves global and App Store submitted `client_context.visible_scope.rows` exactly match the visible `CTOX Zugriff` UI rows, proves Business Chat renders the same submitted scope, and proves persisted `business_commands.client_context.visible_scope.app.module_id` remains the selected app. Evidence: `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/app-store/index.js`; `node --check src/apps/business-os/shared/command-bus.js`; `node --check src/apps/business-os/shared/business-chat.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 18 passed; `node --test src/apps/business-os/shared/command-bus.test.mjs` - 2 passed; `node --test src/apps/business-os/shared/business-chat.test.mjs` - 3 passed; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-agent-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60241 SIGNALING_PORT=60242 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_agent_scope_client_context_matches_ui=1`, `business_os_agent_scope_app_store_context_matches_ui=1`, `business_os_agent_scope_business_chat_scope_matches_context=1` and `business_os_agent_scope_audit_visible=1`; `git diff --check -- src/apps/business-os/modules/app-store/index.js src/apps/business-os/modules/app-store/app-store.test.mjs src/apps/business-os/shared/shell-permissions-ui.js src/apps/business-os/shared/shell-permissions-ui.test.mjs src/apps/business-os/shared/command-bus.js src/apps/business-os/shared/command-bus.test.mjs src/apps/business-os/shared/business-chat.js src/apps/business-os/shared/business-chat.test.mjs src/apps/business-os/app.js src/apps/business-os/app.css src/apps/business-os/modules/coding-agents/index.js src/core/rxdb/tools/browser_rust_smoke.js`. Native/MCP audit metadata parity is closed by `P12-AUDIT-METADATA`. | Closed |
| P12-AUDIT-METADATA | 12 | Native command and MCP audit streams must preserve app/data scope metadata without copying free-form prompts, selections or payloads. | `src/core/business_os/store.rs::record_business_policy_decision_event`, `src/core/business_os/store.rs::policy_audit_client_context`, `src/core/business_os/mcp_channel.rs::argument_metadata_with_policy`, `src/core/business_os/mcp_channel.rs::argument_business_scope_metadata` | PASS 2026-06-18: native `business_events` policy entries now persist a redacted, whitelist-only `client_context` with app/module/record fields and `visible_scope`; free-form prompt and selected text are deliberately excluded. MCP `business_os_mcp_events.metadata_json` now includes `business_scope` with tool, module, action, collection, record and command identifiers while excluding title, objective, query and payload content. Evidence: `rustfmt --edition 2021 src/core/business_os/store.rs src/core/business_os/mcp_channel.rs --check`; `cargo test --bin ctox allowed_policy_decision_writes_business_event_audit -- --nocapture` - 1 passed; `cargo test --bin ctox audited_mcp_policy_denial_records_business_os_policy_decision -- --nocapture` - 1 passed; `cargo test --bin ctox audited_mcp_read_denial_records_business_os_policy_decision -- --nocapture` - 1 passed; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; first post-build Browser/Rust rerun passed all feature keys but failed startup budget (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=60153`); final rerun `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-agent-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60351 SIGNALING_PORT=60352 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with all agent-scope evidence keys true, browser warnings/errors/404/request failures 0 and `startup_smoke_hook_reload_count=0`, `startup_smoke_hook_wait_ms=43`. | Closed |
| P12-AGENT-GRANTS-UI | 12 | Exact agent app/data grants need a business-facing management UI or a documented admin surface; raw grant JSON is not production UX. | `src/apps/business-os/shared/react-settings.js::agentGrantBoundaryPanel`, `src/apps/business-os/shared/react-settings.test.mjs`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsAgentScopeUiSmoke` | PASS 2026-06-18: Settings Admin now renders active explicit grants as `Agent- und App-Zugriff` with business-facing subject, permission and scope labels, including app visibility and collection data grants. The UI deliberately stays read-only because no server-authoritative grant mutation command exists yet; it documents the Owner/Admin policy boundary instead of creating UI-only permission writes. Browser/Rust `business-os-agent-scope-ui` proves the panel is visible after an exact `data.read` grant with `business_os_agent_scope_settings_grant_boundary_visible=1`. Evidence: `node --check src/apps/business-os/shared/react-settings.js`; `node src/apps/business-os/shared/react-settings.test.mjs` - 7 passed; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; Browser/Rust final rerun above. | Closed |
| P13-MODULE-INVENTORY | 13 | Packaged/core module DB-isolation inventory must be committed, complete and owner-assigned before migration starts. | `src/apps/business-os/modules/*/module.json`, `src/apps/business-os/desktop-apps/*/app.js`, `src/apps/business-os/app.js::createLiveDbFacade`, `docs/business-os-db-isolation-inventory.json`, `src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` | PASS 2026-06-17, refreshed 2026-06-18: `docs/business-os-db-isolation-inventory.json` classifies every current packaged/core module, desktop app and Shell facade with classification, owner, review date and migration/exception status. The guard fails on missing/stale modules, desktop apps or unscoped facades and verifies manifest/source metadata plus current raw/property/proxy DB-access shape. Evidence: initial `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; after Phase 13H `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 0 unscoped facades; `node --check src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`; JSON parse guard. | Closed |
| P13-CORE-DB-ISOLATION | 13 | Packaged/core modules must use guarded facades or exact tested scoped exceptions. | `src/apps/business-os/app.js`, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js`, `docs/business-os-db-isolation-inventory.json`, packaged module tests | PASS 2026-06-18: packaged/starter user-module batch migrated for `coding-agents`, `calendar`, `buchhaltung`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `matching`, `notes`, `outbound`, `research`, `shiftflow`, `spreadsheets` and `support`. `createLiveDbFacade(mod)` now enables the guarded DB facade for runtime-installed modules plus this batch; `createModuleContext(mod)` exposes `ctx.permissions`; `ctx.runtimeCapabilities.database` reports guarded raw/property/cached handles for those modules; the Dynamic Apps Browser/Rust smoke matrix requires `business_os_dynamic_packaged_guard_modules=coding-agents,calendar,buchhaltung,conversations,customers,cv-print-builder,documents,invoices,iot,notes,outbound,research,matching,shiftflow,spreadsheets,support`, count 16, guarded capability contracts, collection/property/raw/context denial before `data.read`, `ctx.permissions` deny/allow parity, Support Shell locked-state rendering without data grants, read success after exact collection grants and write denial without `data.write`. Module-owned smoke coverage now includes `coding_agent_sessions`, `calendar_events`, `accounting_journal_entries`, `customer_accounts`, `accounting_invoices`, `iot_widgets`, `matching_requirements`, `notes`, `outbound_campaigns`, `research_tasks` and `planning_shifts`. The user-module batch, formerly unscoped Shell/Desktop facades and scoped system/internal exception work are closed; App Store, Browser, Creator, CTOX, Desktop, Knowledge, Reports and Tickets have exact scoped collection allowlists validated by the inventory guard and Dynamic Apps Browser/Rust smoke. | Closed |
| P13-REAL-SHELL-DB-PATH | 13 | Real Shell dynamic app context must pass active module into `createLiveDbFacade(mod)` and guard collection/cached/raw handles after reload/fresh profile. | `src/apps/business-os/app.js::createModuleContext`, `src/apps/business-os/app.js::createLiveDbFacade`, `src/apps/business-os/app.js::createDynamicAppDataGuard`, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js` | PASS 2026-06-17: `createModuleContext(mod)` calls `createLiveDbFacade(mod)` and Browser/Rust `business-os-dynamic-apps-ui` requires `business_os_dynamic_real_context_collection_denied=1`, `business_os_dynamic_real_context_property_denied=1`, `business_os_dynamic_real_context_cached_denied=1`, `business_os_dynamic_real_context_raw_denied=1` and `business_os_dynamic_real_context_cached_read_grant_allowed=1`. The same smoke now writes a runtime-installed fixture into the installed-app state root, persists it into `business_module_catalog`, reloads the Shell, opens it through real `openModule(mod)` and requires `business_os_dynamic_open_module_reload_mounted=1`, `business_os_dynamic_open_module_collection_denied=1`, `business_os_dynamic_open_module_property_denied=1`, `business_os_dynamic_open_module_cached_denied=1`, `business_os_dynamic_open_module_raw_denied=1`, with browser errors/404/request failures 0. Evidence: `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=240000 BUSINESS_PORT=18989 SIGNALING_PORT=28989 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`. | Closed |
| P13-RAW-DB-LINT | 13 | Module conformance must fail new unscoped `ctx.db.raw`, broad `ctx.db.collections`, collection-property or cached DB-handle bypasses unless a narrow system exception/inventory entry exists. | `src/apps/business-os/scripts/assert-module-conformance.mjs`, `src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`, `docs/business-os-db-isolation-inventory.json`, module sources | PASS 2026-06-17 plus hardened/refreshed 2026-06-18: `assert-module-conformance.mjs` invokes the DB-isolation inventory guard; the inventory guard detects raw DB access, collection property access, `ctx.db.collections` proxy access and cached/exported DB handles and fails if current source shape is not represented in `docs/business-os-db-isolation-inventory.json`. The 2026-06-18 hardening closes optional-chaining and local-alias blind spots: `ctx?.db?.raw`, dynamic `ctx?.db?.[name]`, `ctx?.db?.collections`, and `const db = state.ctx?.db; db?.raw/db?.collections` are now detected. All packaged/core module `db_access` flags must be explicit booleans, including explicit `false` values. Evidence: `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 11 cases; current `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 0 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `node --check src/apps/business-os/scripts/assert-module-conformance.mjs`; `node --check src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`. | Closed |
| P13-DYNAMIC-RUNTIME-SAFETY | 13 | Runtime-installed apps are dynamically imported and mounted in the Shell context; production needs an explicit trusted-code boundary plus guarded network/storage/external-effect capabilities, not only DB facade guards. | `src/apps/business-os/app.js::importBusinessOsModule`, `src/apps/business-os/app.js::openModule`, `src/apps/business-os/app.js::createModuleContext`, `src/apps/business-os/app.js::createRuntimeCapabilityFacade`, `src/apps/business-os/app.js::createLiveDbFacade`, `src/apps/business-os/app.js::createDynamicAppDataGuard`, `src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs`, `src/apps/business-os/scripts/validate-app-module.test.mjs`, `src/core/rxdb/tools/browser_rust_smoke.js::prepareBusinessOsDynamicOpenModuleFixture` | PASS 2026-06-18: runtime-installed modules remain same-origin trusted code, not sandboxed iframe code, but the Shell now exposes an explicit `business-os-runtime-capabilities-v1` contract through `ctx.runtimeCapabilities`: local module asset fetch only, guarded `ctx.db`/raw/property/cached handles, forbidden direct business-data HTTP, forbidden dynamic/bare/remote imports, forbidden browser storage authority, forbidden Shell-global mutation, forbidden Worker/service-worker use and `business_os.chat.task` as the only generated-app command-bus external-effect path. The installed App Creator validator now rejects forbidden network fetch, dynamic import, local/session storage, Shell global state, cached `ctx.db` facade handles, direct CTOX control commands, Worker launches, direct navigation and dynamic evaluators; negative fixtures cover fetch/import/storage/global Shell state/cached DB/external-effect bypasses. Browser/Rust `business-os-dynamic-apps-ui` mounts a persisted runtime-installed app through real `openModule` after reload and requires `business_os_dynamic_runtime_safety_contract=1` plus `business_os_dynamic_runtime_safety_capabilities=1` while keeping collection/property/cached/raw DB denial and browser warnings/errors/404/request failures at 0. Evidence: `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node --check src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs`; `node --check src/apps/business-os/scripts/validate-app-module.mjs`; `node --check src/apps/business-os/scripts/validate-app-module.test.mjs`; `node src/apps/business-os/scripts/validate-app-module.test.mjs` - OK; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/scripts/assert-module-conformance.mjs`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60431 SIGNALING_PORT=60432 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with both runtime-safety keys true and browser warnings/errors/404/request failures 0. | Closed |
| P13-BROWSER-STORAGE-SCOPE | 13 | Shell/App Store/Creator use browser storage for UI/session hints; production must prove storage cannot decide roles, app visibility, release state, audience, tenant or data grants. | `src/apps/business-os/app.js::scopedStorageKey`, `src/apps/business-os/app.js::createStorageScopeFacade`, `src/apps/business-os/app.js::businessOsStorageKeys`, `src/apps/business-os/app.js::readTaskbarPins`, `src/apps/business-os/app.js::persistTaskbarPins`, `src/apps/business-os/app.js::readModuleLayout`, `src/apps/business-os/app.js::persistModuleLayout`, `src/apps/business-os/app.js::readAccountPrefs`, `src/apps/business-os/app.js::writeAccountPrefs`, `src/apps/business-os/app.js::readStoredPairingConfig`, `src/apps/business-os/app.js::writeStoredPairingConfig`, `src/apps/business-os/modules/app-store/index.js::mount`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsDynamicAppsUiSmoke`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsAppAudienceUiSmoke`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsAppReleaseUiSmoke` | PASS 2026-06-18: Shell taskbar pins, module layout, account preferences, Shell column/module resizer widths and Pairing config now use scoped browser-storage keys by workspace/actor where relevant; modules receive `ctx.storageScope` with `business-os-storage-scope-v1`, and App Store pane width uses that facade. Browser/Rust `business-os-dynamic-apps-ui` requires `business_os_dynamic_storage_keys_scoped=1` and `business_os_dynamic_storage_scope_contract=1`; `business-os-app-audience-ui` tampers both the legacy global taskbar key and the active scoped taskbar key and still requires private/preview/restricted apps to stay hidden for the outside actor; `business-os-app-release-ui` still requires release storage-boundary evidence after publish/rollback. Evidence: `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/app-store/index.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/scripts/assert-module-conformance.mjs`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test`; `node src/apps/business-os/scripts/validate-app-module.test.mjs` - OK; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; first dynamic Browser/Rust run passed all feature keys but failed startup budget (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=60780`); final dynamic rerun `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60463 SIGNALING_PORT=60464 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with both storage keys true and browser warnings/errors/404/request failures 0; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-audience-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60471 SIGNALING_PORT=60472 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_app_audience_storage_boundary_checked=1`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-release-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60481 SIGNALING_PORT=60482 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_app_release_storage_boundary_checked=1`. | Closed |
| P13-SCOPED-SYSTEM-FACADES | 13 | Shell/Desktop helper facades must not receive broad naked DB access; each internal surface needs an explicit collection allowlist and browser proof for representative paths. | `src/apps/business-os/app.js::createScopedSystemDbFacade`, `src/apps/business-os/app.js::openSettingsDrawer`, `src/apps/business-os/app.js::openDesktopApp`, `src/apps/business-os/app.js::scheduleBusinessCompanions`, `docs/business-os-db-isolation-inventory.json`, `src/core/rxdb/tools/browser_rust_smoke.js` | PASS 2026-06-18: the naked Shell `createLiveDbFacade()` surfaces for Settings, Desktop app windows, Business Chat and Business Reporter are replaced with `createScopedSystemDbFacade(scopeName, collectionNames)`. Settings receives only module-management/admin collections; Business Chat receives chat, command, queue and attachment file collections; Business Reporter receives report and bug-report collections; Desktop apps receive per-app allowlists (`code-editor`, `explorer`, `file-viewer`) or no DB collections where not used (`browser`, `creator`). Unknown collections return no broad DB handle. Evidence: `node --check src/apps/business-os/app.js`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 0 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; inventory flag query returned `flagged_modules=0`, `unscoped_facades=0`; `node --test src/apps/business-os/shared/react-settings.test.mjs` - 9 passed; `node --test src/apps/business-os/shared/business-chat.test.mjs` - 3 passed; `business-os-agent-scope-ui` passed clean with Business Chat and Settings grant-boundary evidence; `business-os-roles-permissions-ui` passed clean with Settings diagnostics/support evidence; `workspace-large-file-viewer-rust-to-browser` passed clean with the Desktop File Viewer rendering a 1,260,036 byte payload through the scoped desktop facade; `business-os-ui-regression` passed for broad Shell/Desktop interaction paths with browser errors/404/request failures 0 and the known Notes contenteditable/flex browser warning. | Closed |
| P14-AUTH-TENANT-SMOKE | 14 | Production auth/reload claims need real browser smokes without new process-env production toggles; hosted tenant-boundary claims stay separated in `P14-TENANT-BOUNDARY`. | `src/apps/business-os/app.js::loadSession`, `src/apps/business-os/app.js::renderLoginGate`, `src/apps/business-os/app.js::renderProfileDrawer`, `src/core/business_os/server.rs::handle_login_request`, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js`, `src/core/rxdb/tools/business_os_production_smoke_registry.js` | PASS 2026-06-18: `loadSession()` now keeps explicit logout authoritative over stored local pairing unless the page was opened with a fresh URL pairing handoff, and the account drawer writes `ctox.businessOs.loggedOut=1` before `/logout`. Browser/Rust `business-os-auth-scope-ui` proves the real login gate, authenticated reload, account-drawer logout, logged-out reload, protected-access blocking with zero modules loaded, stable local tenant/workspace scope across reload/login, clean initial browser context, forged legacy auth storage not widening scope, and final logged-out state. Evidence: `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-auth-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60721 SIGNALING_PORT=60722 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with all `business_os_auth_*` keys passing, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=55`. | Closed |
| P14-SMOKE-MODES-REGISTERED | 14 | Release, audience, agent-scope, auth-scope and fresh-profile modes must exist in both smoke runner and matrix evidence requirements; docs-only rows are not evidence. | `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js`, `src/core/rxdb/tools/business_os_production_smoke_registry.js` | PASS 2026-06-18: `business-os-app-release-ui`, `business-os-app-audience-ui`, `business-os-agent-scope-ui`, `business-os-auth-scope-ui` and `business-os-fresh-profile-ui` are registered in the runner and matrix evidence requirements. The matrix self-test fails on a missing runner mode, missing matrix mode, missing evidence requirement, missing required evidence key and unsupported evidence mode. Release, audience, agent-scope, auth-scope and fresh-profile are now implemented and passing for their covered browser stories. Evidence: `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-release-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60075 SIGNALING_PORT=60076 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-agent-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60241 SIGNALING_PORT=60242 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK; auth evidence is recorded in `P14-AUTH-TENANT-SMOKE`; fresh-profile evidence is recorded in `P14-VISUAL-LABEL-QA`. Audience evidence is recorded in `P11-AUDIENCE-PERSISTENCE`/`P11-DEEPLINK-LOCKED-STATE`. | Closed |
| P14-VISUAL-LABEL-QA | 14 | Lifecycle/version/privacy chips and disabled reasons must survive desktop and narrow viewport visual QA with full accessible labels across launcher/start-menu, Shell tabs, Appbar, App Store and Settings. | `src/apps/business-os/app.js::renderModuleTab`, `src/apps/business-os/app.js::openAppLifecycleDrawer`, `src/apps/business-os/modules/app-store/index.js::renderCard`, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js` | PASS 2026-06-18: Browser/Rust `business-os-fresh-profile-ui` starts from an empty Chromium profile, waits for authoritative Business OS projection, renders runtime app lifecycle labels and version labels through Shell tabs/start menu/lifecycle drawer and App Store cards, proves App Store disabled reasons via `data-disabled-reason`, verifies desktop and 390px narrow viewport visibility, and proves tampered taskbar/lifecycle browser storage cannot make private/restricted apps visible. Evidence: `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-fresh-profile-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60751 SIGNALING_PORT=60752 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with all `business_os_fresh_profile_*` keys passing, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=48`. | Closed |
| P14-TENANT-BOUNDARY | 14 | Hosted/multi-workspace production claims need explicit tenant boundary evidence; local single-workspace smoke can only claim local scope isolation. | `src/apps/business-os/app.js::readStoredPairingConfig`, `src/apps/business-os/app.js::loadSession`, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js`, `src/core/rxdb/tools/business_os_production_smoke_registry.js`, `docs/ctox-rxdb.md` | PASS 2026-06-18: the current product claim is explicitly `local-workspace-only`; no hosted/multi-workspace claim is made. Browser/Rust `business-os-auth-scope-ui` now injects forged stored pairing/auth data with a different instance/user/signaling endpoint, reloads through the real Shell and requires `business_os_auth_cross_scope_storage_denied=1`, unchanged tenant scope, clean browser context and final logged-out state. The production registry requires `business_os_auth_tenant_scope_claim=local-workspace-only`, so future docs cannot silently imply hosted isolation from local evidence. Evidence: `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-auth-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-auth-tenant-boundary-smoke.json BUSINESS_PORT=61741 SIGNALING_PORT=61742 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_auth_cross_scope_storage_denied=1`, `business_os_auth_tenant_scope_claim=local-workspace-only`, browser warnings/errors/404/request failures 0 and `startup_smoke_hook_wait_ms=66`. | Closed |
| P14-PERF-SCALE-BUDGET | 14 | Production browser claims need budgets for startup, sync, app catalog size, release history, grants and audit volume; small fixture success is not enough. | `src/core/rxdb/tools/browser_rust_smoke_matrix.js::validateModeEvidence`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js::validateBrowserDiagnosticsBudget`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js::validateDurationBudget`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js::validateStartupBudget`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js::evidenceRequirementsForMode`, `src/core/rxdb/tools/business_os_production_smoke_registry.js`, `src/core/rxdb/tools/browser_rust_smoke.js` | PASS 2026-06-18: production evidence requirements now support `maximums` as real budget checks, and Fresh Profile requires representative scale evidence. Browser/Rust `business-os-fresh-profile-ui` seeds 32 scale apps, 64 explicit grants, 96 native module-version rows and 128 native `business_events`, then renders 57 catalog apps and 32 App Store cards through the real Shell/App Store path. The registry enforces minimum counts and maximum UI budgets; the accepted run passed with render 8 ms, start menu 10 ms, App Store 116 ms, sync-config wait 11 ms, startup hook wait 36 ms, browser warnings/errors/404/request failures 0 and `business_os_fresh_profile_scale_budget_passed=1`. Evidence: `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-fresh-profile-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-fresh-profile-scale-smoke.json BUSINESS_PORT=61751 SIGNALING_PORT=61752 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with the scale evidence keys above. | Closed |
| P15-WHY-DIAGNOSTICS | 15 | Operator must be able to answer why a selected actor can/cannot see, open, edit, release, rollback or read/write each data area. | `src/apps/business-os/shared/shell-permissions-ui.js::buildModuleWhyDiagnosticsView`, `src/apps/business-os/shared/shell-permissions-ui.js::renderModuleWhyDiagnosticsHtml`, `src/apps/business-os/app.js::openAppLifecycleDrawer`, `src/core/business_os/store.rs::business_os_why_diagnostics`, `src/core/business_os/store.rs::accept_rxdb_business_command`, `src/apps/business-os/shared/react-settings.js::loadModuleWhyDiagnostics`, `src/apps/business-os/shared/react-settings.js::nativeWhyDiagnosticsView`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsRolesPermissionsUiSmoke`, policy evaluator, catalog projection | CLOSED 2026-06-18: Shell lifecycle drawer UI/browser path renders business-facing explanations for actor, visibility/open/edit/source/release/rollback and per-data-area read/write decisions without raw grant JSON, and `business-os-dynamic-apps-ui` requires `business_os_dynamic_lifecycle_why_diagnostics_visible=1`, rows and data evidence. Native `ctox.business_os.why` returns lifecycle/policy diagnostics for private 0.x, explicit `apps.view`, 1.0.0 team visibility, action permissions and data read/write decisions; tests prove prompt/token/selected-text markers are not returned or persisted in the sanitized diagnostics command projection. Settings module management has both static and live Browser/Rust evidence: `business-os-roles-permissions-ui` requires `business_os_roles_permissions_settings_why_diagnostics_visible=1`, row coverage and redaction, with browser warnings/errors/404/request failures 0 and startup reload count 0 after Desktop reload-hardening. The native support diagnostics artifact includes a sanitized Why summary; broader support export/browser smoke remains tracked by `P15-SUPPORT-ARTIFACT-SCHEMA`. | Complete |
| P15-OPS-RECOVERY | 15 | Operators need diagnostics, redacted audit export, retention and recovery drills for release/access failures. | `src/core/business_os/store.rs::business_os_audit_retention_export`, `src/core/business_os/store.rs::business_os_audit_retention_policy_set`, `src/core/business_os/store.rs::business_events_before`, `src/core/business_os/store.rs::prune_business_events_before`, `business_events`, MCP audit, Settings Activity | PASS 2026-06-18: native `ctox.business_os.audit.retention` is gated through the existing `business_commands` path with `users.manage`, writes a support-safe `ctox.business_os.audit_retention_export.v1` artifact under `runtime/business-os/audit-exports` before optional `business_events` prune, stores only a sanitized command projection and denies Teammitglied/spoofed-client-context attempts without creating an export. Native retention policy is now typed persisted Business OS state via `business_os.audit_retention_policy.v1`; `ctox.business_os.audit.retention_policy.set` is `users.manage` gated, validates 1-3650 days, stores a sanitized command projection, and `ctox.business_os.audit.retention` uses the policy when a request omits `retention_days`. Rust tests cover export-before-prune, redaction, current-row preservation, prune result, denied actor/no-export behavior, persisted policy fallback, policy-set gating and prompt redaction. Broader repair boundaries are closed in `P15-REPAIR-COMMANDS`; native backup/restore remains partial in `P15-BACKUP-RESTORE-DRILL` for hosted/cross-version/raw-backup boundaries. | Complete |
| P15-MCP-RETENTION-CONTRACT | 15 | MCP env-policy retention must be migrated, documented as legacy or excluded from production-ready claims. | `src/core/business_os/mcp_channel.rs::mcp_policy`, `src/core/business_os/mcp_channel.rs::save_mcp_policy`, `src/core/business_os/mcp_channel.rs::audit_retention_prunes_expired_mcp_events`, `src/core/service/business_os.rs::handle_business_os_mcp_policy` | CLOSED 2026-06-18: MCP policy now persists as typed payload `business_os.mcp_policy.v1` through `save_mcp_policy`; `mcp_policy(root)` prefers typed native policy state and reads `CTOX_BUSINESS_OS_MCP_*` runtime-env values only as migration fallback. `ctox business-os mcp policy set` writes typed state, returns the typed storage marker and leaves legacy runtime-env values untouched. MCP audit pruning now uses typed `audit_retention_days`, while the legacy fallback test proves existing installations still read old runtime-env policy until a typed policy exists. Rollout docs identify Runtime-Env MCP keys as legacy fallback, not production control surface. Evidence: `rustfmt --edition 2021 --check src/core/business_os/mcp_channel.rs src/core/service/business_os.rs`; `cargo test --bin ctox mcp_policy --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 11 passed; `cargo test --bin ctox audit_retention_prunes_expired_mcp_events --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 1 passed; `cargo test --bin ctox mcp_business_os_policy --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 18 passed. | Complete |
| P15-REPAIR-COMMANDS | 15 | Stale catalog, orphan private app, bad grant, manifest/release-row divergence and broken rollback target must have tested recovery commands or documented manual repair boundaries. | `src/core/business_os/store.rs::repair_module_lifecycle_projections`, `src/core/business_os/store.rs::repair_stale_module_permission_grants`, `src/core/business_os/store.rs::repair_invalid_module_release_version_refs`, `src/core/business_os/store.rs::repair_orphan_private_app_responsibility`, `src/core/business_os/store.rs::rollback_module_to_version`, `src/core/business_os/store.rs::module_lifecycle_projection_repair_safe_command`, rollout/recovery docs | PASS 2026-06-18: existing `ctox.module.repair_lifecycle_projection` has a dry-run mode, explicit action list, no-mutation dry-run test, sanitized command projection and apply test restoring missing `business_records`/RxDB release projections plus catalog projection. It also accepts `repair_stale_grants` and deactivates active module-scoped permission grants whose `scope_id` no longer matches a current module manifest; the focused test proves dry-run reports `apply=false` without mutation, apply sets the stale grant inactive with repair reason, and a valid module grant stays active. `repair_invalid_version_refs` reports and clears release snapshot `source_version_id`/`rollback_version_id` values whose referenced module source version no longer exists before regenerating the release projection. `repair_orphan_private_apps` reports private runtime `0.x` apps without active App-Verantwortliche and, in apply mode, assigns the current Admin/Owner actor through the existing audited `business_module_acl` path before regenerating catalog lifecycle projection. `ctox.module.rollback_version`/`rollback_module_to_version` now has explicit test evidence for restoring `module.json`, editable source files and removing files added after the target source version. This closes the Phase-15 repair-command scope for stale release/catalog projection, stale module grants, broken version refs, orphan private-app responsibility and source-version manifest/source rollback. Missing source-version history remains a backup/restore boundary, not a repair-command promise. | Complete |
| P15-BACKUP-RESTORE-DRILL | 15 | Runtime store, module manifests, source snapshots, release rows, audit events and retention settings need tested backup/restore or documented manual recovery before production rollout. | `src/core/business_os/store.rs::run_business_os_backup_restore_drill`, `src/core/business_os/store.rs::business_os_create_portable_backup_export`, `src/core/business_os/store.rs::business_os_verify_portable_backup_export`, `src/core/business_os/store.rs::inspect_business_os_backup_manifest`, `src/core/business_os/store.rs::inspect_business_os_backup_key_escrow`, `src/core/business_os/store.rs::business_os_backup_restore_version_compatibility`, `src/core/business_os/store.rs::prune_business_os_backup_restore_drills`, `src/core/business_os/store.rs::business_os_backup_restore_drill_export`, `src/core/business_os/store.rs::business_os_active_root_restore_runbook`, `src/core/business_os/store.rs::business_os_restore_drill_service_state`, `src/core/service/business_os.rs::handle_business_os_backup`, `src/core/rxdb/tools/browser_rust_smoke.js`, `runtime/backup/business-os-drill-*`, `runtime/business-os/restore-drills`, rollout/operator docs | PASS 2026-06-18: CLI-only `ctox business-os backup restore-drill [--module <id>]` creates online SQLite snapshots for core state, CTOX Secret Store, Business OS and native RxDB stores, copies installed module manifests/assets, source snapshots and audit exports, restores into an isolated `restore-root`, runs SQLite integrity checks, writes a hash manifest and validates support-safe restored-state facts for typed MCP policy, typed native audit-retention policy, release rows, rollback target, installed manifests, source snapshots, audit exports and RxDB catalog projection. The manifest now includes `raw_backup_security` with `support_attachment_allowed=false`, local retention days and encrypted-portable-export requirement, `restore_compatibility` with same-version support plus downgrade/cross-version block policy, `manifest_integrity` with Secret-Store-backed HMAC-SHA256 signature evidence and `portable_encrypted_export` with chunked AES-256-GCM snapshot ZIP metadata. The portable export is encrypted with a Secret-Store-backed key, writes no key material into the manifest, verifies by decrypting the ciphertext, matching plaintext size/SHA-256, opening the ZIP and matching entries against the snapshot manifest, then deletes temporary plaintext/verify ZIPs. `ctox business-os backup inspect-manifest --manifest <path>` now performs a non-destructive restore preflight: manifest HMAC verification, supported schema check, same-version compatibility decision, automatic cross-version/downgrade block and portable ciphertext hash check. `ctox business-os backup key-escrow-status` now reports redacted machine-readable key-escrow readiness: before a drill it reports the missing portable key without generating it; after a drill it reports Secret-Store presence, key fingerprint and external escrow requirements without printing the raw key. `ctox business-os backup prune-drills [--dry-run]` reports and deletes only expired `business-os-drill-*` directories whose manifest carries retention expiry; missing retention metadata is reported but not deleted. Native `ctox.business_os.backup.restore_drill` is a `runtime.manage` gated support-safe preflight with sanitized command projection; it does not create raw backups. Raw CLI backups contain sensitive data and are not support attachments. The drill and preflight expose `active_root_restore_runbook` with `destructive_restore_performed=false`, explicit quiesce/restart gates, manifest SHA/signature verification, portable-export verification, key-escrow confirmation and restore targets for core state, Secret Store, Business OS/native RxDB stores, installed app roots, source snapshots and audit exports. The focused restore-drill test asserts the signed manifest, Secret-Store snapshot, portable encrypted export, decrypt/ZIP verification, key redaction, temp plaintext deletion, key-escrow status redaction, inspect-manifest preflight, cross-version/downgrade block decisions, runbook gates, active-root boundary, redaction and browser IndexedDB/hosted WebRTC boundaries, plus prune dry-run/apply behavior. Separate Browser/Rust `business-os-restore-resync-ui` now proves the local same-profile browser IndexedDB case: while the native peer is stopped, a browser-local desktop file/chunk write does not reach native SQLite; after peer restart it refreshes WebRTC checkpoint epochs and converges to native SQLite with warning/error/request-failure counts 0. Remaining blockers: hosted/multi-workspace WebRTC restore proof and external key-escrow operational signoff; release-level cross-version/downgrade evidence now has a native blocking preflight but still needs real release-to-release restore rehearsal before claiming automatic compatibility. Evidence: `rustfmt --edition 2021 --check src/core/business_os/store.rs src/core/service/business_os.rs src/core/secrets.rs`; `cargo test --bin ctox backup_restore_drill --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 4 passed; `cargo check --bin ctox --no-default-features --target-dir runtime/build/business-os-backup-check-target`; six-mode production Browser/Rust matrix passed on 2026-06-18 with `business-os-restore-resync-ui`. | In progress |
| P15-SUPPORT-ARTIFACT-SCHEMA | 15 | Support diagnostics need a stable redacted artifact schema so operators can attach evidence without leaking prompts, record bodies, message bodies, tokens or secrets. | `src/core/business_os/store.rs::business_os_support_diagnostics_export`, `src/core/business_os/store.rs::support_diagnostics_activity_summary`, `src/core/business_os/store.rs::support_diagnostics_why_summary`, `src/apps/business-os/shared/react-settings.js::exportSupportDiagnosticsArtifact`, `src/apps/business-os/shared/react-settings.js::moduleSupportDiagnosticsHtml`, `src/core/rxdb/tools/browser_rust_smoke.js::runBusinessOsRolesPermissionsUiSmoke`, Diagnostics command path, Settings Activity, MCP audit export | CLOSED 2026-06-18: native `ctox.business_os.support.export_diagnostics` returns `ctox.business_os.support_diagnostics.v1` with `support-safe-v1` redaction manifest, actor/scope, Activity summaries without raw event payloads, optional sanitized Why summary and sanitized command projection; Rust test proves prompt/token/selected-text/message-body/record-payload markers do not leak. Settings module management now exposes a per-app `Support-Paket` action, dispatches the native command, renders only business-facing schema/protection/scope/activity/why rows, avoids raw policy keys and sensitive marker text in visible UI, and offers a JSON download link. Browser/Rust `business-os-roles-permissions-ui` requires `business_os_roles_permissions_settings_support_diagnostics_visible=1`, rows, redaction and download evidence with browser warnings/errors/404/request failures 0 and startup reload count 0. Native audit export-before-prune is tracked by `P15-OPS-RECOVERY`/15B rather than this schema item. | Complete |
| P16-CI-RELEASE-GATES | 16 | CI/release process must fail on missing smoke modes, undocumented skips or stale release controls. | `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js`, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/business_os_production_smoke_registry.js`, `src/apps/business-os/app.js` | CLOSED 2026-06-18: the Linux x86_64 CI path now installs the declared Business OS JS bootstrap, runs npm audit/shared/App Store/module-bundle tests, runs required smoke-mode self-test, then executes `businessOsProductionSmokeModes` with `SMOKE_MATRIX_ATTEMPTS=2`, fixed `SMOKE_PAGE_PATH=/index.html`, `SMOKE_BROWSER_WARNING_BUDGET=0`, `SMOKE_BROWSER_REQUEST_FAILURE_BUDGET=0` and uploads `runtime/build/business-os-smoke-matrix-summary.json`. The local CI-equivalent gate passed after hardening volatile data-plane-shutdown catalog refresh logging from warning to debug and extending production evidence with auth state, actor role, browser context and tenant scope. The current registry contains six modes: Release, Audience, Agent Scope, Auth Scope, Fresh Profile and Restore/Resync. Evidence: `npm ci --ignore-scripts --prefix src/apps/business-os`; `npm audit --audit-level=low --prefix src/apps/business-os`; `npm --prefix src/apps/business-os test`; `npm --prefix src/apps/business-os run test:module-bundles`; `cargo build --locked --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`; `node --check src/apps/business-os/app.js src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js src/core/rxdb/tools/business_os_production_smoke_registry.js src/apps/business-os/scripts/assert-business-os-js-bootstrap.mjs src/apps/business-os/modules/ctox/test.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; final Browser/Rust `SMOKE_MODES="$(node -e "const { businessOsProductionSmokeModes } = require('./src/core/rxdb/tools/business_os_production_smoke_registry'); process.stdout.write(businessOsProductionSmokeModes.join(','));")" SMOKE_MATRIX_ATTEMPTS=2 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_BROWSER_WARNING_BUDGET=0 SMOKE_BROWSER_REQUEST_FAILURE_BUDGET=0 BUSINESS_PORT=61951 SIGNALING_PORT=61952 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` passed all six production modes with browser warnings/errors/request failures/asset errors/startup reloads 0; artifact validation confirmed schema `ctox.business_os.smoke_matrix_summary.v1`, `requestedAttempts=2`, all six URLs and complete context fields for every mode. | Complete |
| P16-RELEASE-WORKFLOW-GATE | 16 | Tag/release artifact upload must depend on the Business OS production gate job. | `.github/workflows/release.yml`, `.github/workflows/ci.yml` | CLOSED 2026-06-18: `.github/workflows/release.yml` now adds `business-os-production-gate` before artifact-producing jobs. It runs the same JS bootstrap, RxDB/static/module conformance, Rust Business OS/RxDB tests, warning-clean production Browser/Rust smoke matrix and uploads `business-os-release-production-smoke-evidence`. Artifact-producing release jobs `build-desktop-macos`, `build-desktop-linux`, `build-desktop-windows`, `build-business-os-desktop` and `build-ctox` all declare `needs: business-os-production-gate`, and the final `release` job also needs the gate. Local Ruby workflow validation returned `business_os_release_gate_upload_dependency=1` and listed `build-desktop-macos,build-desktop-linux,build-desktop-windows,build-business-os-desktop,build-ctox`, proving no release artifact upload job except the gate's own evidence upload can run without that dependency. | Complete |
| P16-SMOKE-ARTIFACT | 16 | Smoke matrix summary must be written to a fixed path and schema-validated. CI upload/retention is tracked by `P16-CI-RELEASE-GATES`. | `src/core/rxdb/tools/browser_rust_smoke_matrix.js` | PASS 2026-06-18: `browser_rust_smoke_matrix.js` writes `runtime/build/business-os-smoke-matrix-summary.json` by default with schema `ctox.business_os.smoke_matrix_summary.v1`, schema version, repository root, git revision, binary path, page path, requested modes, parsed configuration, start/end timestamps, mode attempts, browser URL, auth/role/profile/tenant context, evidence keys, full evidence, warning budgets and result. The validator fails malformed summaries before writing a successful artifact, requires at least one accepted final attempt per mode, and for production modes rejects missing or empty auth/role/profile/tenant context, missing advanced status and successful attempts that exceed their recorded budgets. Self-test covers missing git revision, missing attempt URL, missing configuration, evidence-key drift, production-context drift and production-budget drift. Evidence: `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; first `business-os-agent-scope-ui` Browser/Rust run with explicit result path passed feature keys but failed startup budget (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=61751`), so it was not accepted; rerun with explicit result path passed clean; final default-path Browser/Rust run `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-agent-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61441 SIGNALING_PORT=61442 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0`, `startup_smoke_hook_wait_ms=81`; JSON validation confirmed fixed absolute `resultPath`, git revision `3c33b8cbc24ae08fd31be857a2fecae0519cb83c`, URL `http://127.0.0.1:61441/index.html`, `authState=authenticated`, `actorRole=user`, `browserContext=clean`, `tenantScope=local-workspace`, 36 evidence keys and warning budget fields. | Closed |
| P16-SMOKE-REQUIRED-MODES-GUARD | 16 | CI must fail when required Business OS production smoke modes are missing from runner/matrix or when a matrix row lacks required evidence keys. | `src/core/rxdb/tools/browser_rust_smoke_matrix.js`, `src/core/rxdb/tools/business_os_production_smoke_registry.js`, `.github/workflows/ci.yml` | PASS 2026-06-17, hardened 2026-06-18: `.github/workflows/ci.yml` syntax-checks `business_os_production_smoke_registry.js` and runs `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` in the existing x86_64 Linux Business OS RxDB-only contract job. The self-test fails on missing runner mode, matrix mode, evidence requirement, required evidence key, unsupported evidence mode, final production context drift and final production budget drift; Auth/Fresh-Profile modes now also require auth/profile/role/tenant context evidence. | Closed |
| P16-LEGACY-FIXTURES | 16 | Migration/backfill gates require source-committed fixtures for private, missing, invalid, released, restricted and preview apps. | `src/core/business_os/store.rs::module_catalog_projects_runtime_app_lifecycle_backfill`, `src/core/business_os/store.rs::backfill_manifest_preview_audience_grants`, `src/core/business_os/store.rs::projected_module_lifecycle`, `src/core/business_os/store.rs::module_catalog_for_rxdb`, `src/apps/business-os/shared/app-lifecycle.test.mjs` | CLOSED 2026-06-18: the existing inline Rust temp-root lifecycle backfill test now covers private `0.x`, missing version, invalid SemVer, released `1.x`, restricted and preview legacy manifests. It proves only expected `apps.view` grants are inserted for preview/restricted users, no private/missing/invalid/released app becomes more public, no `data.read`/`data.write` grant is created, and a second `module_catalog_for_rxdb` run after a partial existing grant is idempotent. Evidence: `rustfmt --edition 2021 --check src/core/business_os/store.rs`; `cargo test --bin ctox module_catalog_projects_runtime_app_lifecycle_backfill --no-default-features --target-dir runtime/build/core-rxdb-integration-target`; `cargo test --bin ctox business_app_semver_major_matches_browser_plain_semver_contract --no-default-features --target-dir runtime/build/core-rxdb-integration-target`; `node --test src/apps/business-os/shared/app-lifecycle.test.mjs`. | Complete |
| P16-JS-BOOTSTRAP | 16 | Clean-checkout JS tests must have a declared dependency bootstrap; no gate may rely on local symlinks or host-global packages. | `src/apps/business-os/package.json`, `src/apps/business-os/package-lock.json`, `.github/workflows/ci.yml`, `src/apps/business-os/scripts/assert-business-os-js-bootstrap.mjs`, `src/core/rxdb/tools/browser_rust_smoke.js`, Business OS JS/module tests | PASS 2026-06-18: Business OS now has a private test-bootstrap package with exact dev pins for `esbuild@0.28.1` and `playwright@1.60.0`, a committed lockfile, and a guard that verifies package/lock pins plus real module resolution and APIs from `src/apps/business-os/node_modules`. The package is `type: module`, the previous CommonJS CTOX module test is ESM-compatible, and `browser_rust_smoke.js` can resolve Playwright from the declared Business OS bootstrap path without a host-global install. CI now runs `npm ci --ignore-scripts --prefix src/apps/business-os`, `npm audit --audit-level=low --prefix src/apps/business-os`, `npm --prefix src/apps/business-os test` and `npm --prefix src/apps/business-os run test:module-bundles` on the existing Linux x86_64 branch. Evidence: `npm ci --ignore-scripts --prefix src/apps/business-os` - installed 5 packages, 0 vulnerabilities; `npm audit --audit-level=low --prefix src/apps/business-os` - 0 vulnerabilities; `npm --prefix src/apps/business-os run check:deps` - `business_os_js_bootstrap=1`, esbuild 0.28.1, Playwright 1.60.0; `npm --prefix src/apps/business-os test` - 48 shared tests and 18 App Store tests passed without module-type warnings; `npm --prefix src/apps/business-os run test:module-bundles` - Calendar 7 tests, CTOX 9 checks, Customers schema smoke, IoT contract, Notes 4 tests, Outbound 10 tests and Research 5 tests passed; `node --check src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js src/apps/business-os/modules/ctox/test.js src/apps/business-os/scripts/assert-business-os-js-bootstrap.mjs`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `git diff --check -- .github/workflows/ci.yml src/apps/business-os/package.json src/apps/business-os/package-lock.json src/apps/business-os/modules/ctox/test.js src/apps/business-os/scripts/assert-business-os-js-bootstrap.mjs src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js`. | Closed |
| P16-SECURITY-PRIVACY-SIGNOFF | 16 | Full production-ready claim needs explicit security/privacy signoff for dynamic app code, source visibility, data review, MCP/agent scopes, audit export, external effects and release artifact integrity. | `docs/business-os-security-privacy-signoff.json`, `docs/business-os-production-release-signoff.md`, `src/apps/business-os/scripts/assert-security-privacy-signoff.mjs`, `.github/workflows/release.yml`, `.github/workflows/ci.yml`, `src/apps/business-os/app.js`, `src/core/business_os/mcp_channel.rs`, `src/core/business_os/store.rs`, rollout docs | IN PROGRESS 2026-06-18: a structured `ctox.business_os.security_privacy_signoff.v1` JSON artifact now lists all required security/privacy controls, evidence refs and source-hash fields. Normal CI validates the schema, required controls, evidence paths, workflow wiring and self-test without requiring human signoff. The tag-release workflow runs `node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs --require-signed-off` after the production Browser/Rust smoke and uploads `runtime/build/business-os-security-privacy-signoff-validation.json`; release tags intentionally fail while the JSON artifact is `pending-signoff`, controls are pending and source hashes are not recorded. The Markdown signoff file remains the human checklist companion. Actual security/privacy signoff remains open until the JSON controls are signed off, reviewer/date/evidence revision and matching source hashes are recorded, and the release commit evidence is reviewed. Evidence: `node --check src/apps/business-os/scripts/assert-security-privacy-signoff.mjs`; `node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs --self-test`; `node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs` passed with `status=pending-signoff`; `node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs --require-signed-off` failed as expected and wrote a validation artifact with `ok=false`. | Open |
| P16-CUSTOMER-OPERATOR-DOCS | 16 | Customers and operators need business-facing docs for roles, lifecycle labels, publish/review, preview/restricted sharing, agent scopes, locked data states and recovery. | `docs/business-os-app-access-and-roles-guide.md`, `docs/business-os-roles-permissions-operator-guide.md`, `docs/business-os-roles-permissions-rollout.md`, `src/apps/business-os/scripts/assert-production-release-docs.mjs`, Settings/App Store copy | IN PROGRESS 2026-06-18: `docs/business-os-app-access-and-roles-guide.md` now gives customer-facing current behavior for roles, app visibility/version labels, `Freigeben`, read-only Settings release diagnostics, data grants/locked states, agents and diagnostics. `docs/business-os-roles-permissions-operator-guide.md` documents the operator side: current role labels, `0.x`/`1.0.0` lifecycle visibility, app badge semantics, App Store publish/review, rollback, preview/restricted sharing, agent/MCP scope, locked data areas, Why diagnostics, Support-Paket and recovery boundaries. The rollout guide links both guides plus the signoff artifacts and requires `runtime/build/business-os-release-docs-dry-run.json`. CI validates required doc labels/sections through `assert-production-release-docs.mjs`; the release workflow reruns it after the production browser gate and uploads the dry-run artifact with UI-source anchors and smoke summary metadata. Remaining work: final human release review/signoff. Evidence: `node src/apps/business-os/scripts/assert-production-release-docs.mjs` passed with `business_os_release_docs_ok=1` and wrote `runtime/build/business-os-release-docs-dry-run.json`. | Open |

## Cross-Cutting Product Gates

These gates apply to every phase that touches visible permissions, dynamic
apps, release state, data access or agents.

- Accessibility: lifecycle badges, menus, dialogs, disabled reasons and
  diagnostics must be keyboard reachable, have stable focus behavior and expose
  business-facing labels to assistive tech.
- Responsive UI: Shell icons, App Store cards/details, Settings fallbacks and
  release/audience drawers must fit desktop and narrow widths without text
  overlap or hidden critical controls.
- Loading/empty/error states: permission-denied data areas, missing release
  metadata, WebRTC/RxDB startup delay, failed command outcome and unavailable
  auth state must render intentional states, not raw exceptions or stuck
  spinners.
- Multi-tab/reload behavior: permission, audience, release and rollback changes
  must settle consistently after reload and in a second browser context.
- Concurrency: release, rollback, audience and data-review actions must define
  behavior for stale version rows, duplicate commands and in-flight updates.
- Privacy and redaction: diagnostics and audit exports must avoid prompt
  bodies, record payload bodies, secrets, tokens and sensitive field values by
  default.
- Internationalization boundary: user-facing labels stay business German for
  this rollout; raw stored role names and permission identifiers are allowed
  only in developer diagnostics, tests and logs.
- Backward compatibility: existing installs with invalid/missing SemVer,
  private `0.x`, released `1.x`, preview/restricted metadata or legacy release
  rows must migrate without widening app visibility or data grants.

## Production-Ready Coverage Map

This map is the completeness checklist for the remaining work. Each row must
have at least one closed blocker-ledger item, implementation evidence, browser
evidence and release-gate evidence before `Full product production-ready` can
be claimed.

| Ring | Production question | Owning phases | Required proof |
| --- | --- | --- | --- |
| Authority | Does the backend, not the UI, decide who may view, edit, release, rollback, grant, approve or delegate? | 10-12, 15 | Rust policy tests, failed-command outcomes, audit events and spoofed-client-context tests |
| App lifecycle | Are `0.x` private/preview, `1.0.0+` Team and restricted states persisted, projected and visible after reload/fresh profile? | 10-11, 14 | Native projection tests, Shell/App Store/Settings UI tests and Browser/Rust reload/deep-link smokes |
| App Store UX | Can a non-technical App-Verantwortliche:r publish, review data, select rollback and understand errors without raw IDs? | 10, 14, 16 | App Store hook tests, browser publish/rollback story, visual/accessibility checks and customer docs |
| Data access | Does app visibility stay separate from actual data grants, including locked states? | 10, 12-14 | Data-review reconciliation, runtime DB facade tests, locked-state UI tests and agent data-scope tests |
| Runtime safety | Can installed/generated app code bypass data, storage, network or external-effect boundaries? | 13-14, 16 | Runtime capability inventory, negative generated-app fixtures, static guard and Browser/Rust runtime-safety smoke |
| Audience | Can preview/restricted audiences be managed durably without abusing edit/source/release grants? | 11, 14-15 | Native audience contract, migration tests, browser deep-link/fresh-profile smoke and audit/recovery evidence |
| Agent parity | Does an AI/service actor see exactly the same app/data boundary as a human actor, with visible scope before action? | 12, 14-15 | MCP tests, in-app scope panels, client-context integrity tests and delegated-action audit evidence |
| Auth/tenant | Does login/logout/reload/fresh-profile and tenant/workspace isolation hold in the real browser? | 14, 16 | Registered smoke modes, clean-profile artifacts, negative cross-scope tests and no process-env production toggles |
| Sync/concurrency | Can WebRTC/RxDB delay, offline, duplicate command or stale catalog state produce false success? | 10, 14-15 | Projection-failure tests, duplicate-command tests, repair commands, smoke diagnostics and warning budgets |
| Observability | Can support explain every allow/deny/release/rollback/audience/data/agent decision without raw DB inspection? | 15 | Why diagnostics, Activity UI, redacted support artifact and export-before-prune tests |
| Migration | Do existing installs stay conservative across private, invalid, released, restricted and preview app states? | 11, 15-16 | Committed legacy fixtures, idempotent migration tests and downgrade/runbook evidence |
| Operations | Can an operator backup, restore, repair, prune, roll back and document the rollout safely? | 15-16 | Backup/restore drills, recovery commands, retention config, support export and operator dry-run |
| Performance | Does the product remain usable with representative app catalogs, grants, release history and audit volume? | 14, 16 | Scale fixtures, startup/sync/mode duration budgets, visible progress/error states and CI budget enforcement |
| Release | Can artifacts ship only after all relevant gates pass, with machine-readable evidence? | 16 | Required CI job, fixed smoke artifact schema, release workflow dependency and artifact upload proof |
| Security/privacy | Are app source, dynamic app code, agent context, audit export, external effects and release artifacts reviewed as one boundary? | 12, 15-16 | Threat-model/security checklist, redaction proof, external-effect boundary tests and release signoff |

Terminology:

- `Locally production-ready core slice`: current Phase 0-9 status; verified
  locally for the implemented role/lifecycle/data-facade core.
- `Full product production-ready`: only after Phases 10-16 are `Complete`.
- `Backend ready`: native/Rust policy and persistence checks passed, but real
  browser UX is not proven.
- `Not production ready`: any required user story, auth/tenant check,
  WebRTC/RxDB health check or release/audit gate is missing or failing.

## Production Implementation Slices

Phases 10-16 must be implemented as independently testable slices. A slice can
ship into the working tree only when its own backend/static tests pass and the
plan is updated with source anchors, evidence and residual risk. Browser-facing
slices cannot be marked complete until the relevant Browser/Rust smoke mode is
registered and run.

### Phase 10 Slices

| Slice | Goal | Depends on | Minimum local gate | Exit state |
| --- | --- | --- | --- | --- |
| 10C No legacy HTTP module source/release/rollback | Disable active legacy HTTP routes and add a source guard so App Store/Settings can only use RxDB/WebRTC commands for source, release and rollback. | Phase 10A/10B backend commands | `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; targeted Rust compile/test for server/store command paths | `P10-BE-NO-HTTP-REENABLE` closed |
| 10D1 Release write-order and stale-version guard | Validate release `source_version_id`/`rollback_version_id` before manifest writes, write release/source-version summaries transactionally and restore `module.json` on injected release/rollback DB failures. | 10C | Rust failure-injection tests for stale version refs, release DB failure and rollback DB failure | Backend sub-slice passed; later 10D2/10E2 close the remaining backend consistency/projection work |
| 10D2 Projection/repair consistency closure | Define and test duplicate command behavior, stale release projection repair, catalog repair and the handoff boundary to Phase-15 operator recovery. | 10D1 | Rust duplicate-command tests plus repair/diagnostic command test for canonical release projection, RxDB release row and catalog repair | Passed; `P10-BE-CONSISTENCY` closed and Phase 15 owns broader operator recovery/support drills |
| 10E1 Release lifecycle audit events | Write queryable `business_events` for successful release, successful rollback, failed release validation and failed rollback outcome, with redacted business-facing summaries and Activity command coverage. | 10D1 | Rust audit tests plus Activity-list command test for the new event types | Passed; Settings/browser evidence is covered by `business-os-app-release-ui`, and `P10-BE-AUDIT` is closed |
| 10E2 Release projection backend | Project release review/current state, rollback target and granted/locked data-area summary to catalog lifecycle after release and rollback. | 10D1/10E1 | Rust projection/backfill tests for release, rollback, data-access summary and runtime-installed manifest root consistency | Backend sub-slice passed; later Browser/Rust release smoke closed `P10-BE-RELEASE-PROJECTION` |
| 10E3 Release projection reload closure | Prove Shell/App Store/Settings consume projected release state after reload and do not render success from command result alone. | 10E2/10D2 | Browser/Rust reload evidence plus UI tests for projected `release_state`, `rollback_target` and data-area summary | UI/static consumption passed for Shell, App Store and Settings; Browser/Rust App Store reload/success-wait evidence passed; `P10-BE-RELEASE-PROJECTION` closed |
| 10F App Store publish wizard | Implement business-facing App Store flow with version, snapshot, release notes, data review, responsible users, rollback target and final summary. | 10E2 | App Store hook tests, JS permission tests, no raw grant/manifest UI guard | UI/payload and Browser/Rust release smoke passed; `P10-UI-WIZARD` closed |
| 10G Settings fallback alignment | Either move Settings release controls to the Phase-10 payload or downgrade them to read-only/expert diagnostics. | 10F or earlier read-only downgrade | Settings tests plus static guard for stale publish controls plus Browser/Rust real Settings drawer evidence | Read-only downgrade passed and Browser/Rust proved disabled diagnostics; `P10-SETTINGS-FALLBACK` closed |
| 10H Release browser smoke | Register and pass `business-os-app-release-ui` with private app publish, Team reload visibility, data-area UI, rollback and diagnostics budgets. | 10F/10G | Browser/Rust smoke matrix with fixed evidence keys and zero unbudgeted errors | Passed with all release and Settings Activity audit evidence keys true and browser warning/error/request failure counts 0; Phase 10 is complete |

### Phase 11 Slices

| Slice | Goal | Depends on | Minimum local gate | Exit state |
| --- | --- | --- | --- | --- |
| 11A Visibility state contract | Choose and implement durable native visibility/audience semantics separate from edit/source/release/data grants. | Phase 10 complete | Rust model/projection tests, JS lifecycle/permission tests and Browser/Rust dynamic-app smoke | `P11-AUDIENCE-GRANT` closed for the dedicated visibility grant |
| 11B Audience persistence and route lock | Migrate legacy preview hints to durable `apps.view`, project audience only from grants, and block direct hidden-app opens before import. | 11A | Rust migration/projection tests, JS lifecycle tests and Browser/Rust `business-os-app-audience-ui` | `P11-AUDIENCE-PERSISTENCE` and `P11-DEEPLINK-LOCKED-STATE` closed |
| 11C Responsibility safety | Add reassignment/orphan prevention for private apps and audit responsibility changes. | 11B | Rust command tests and Activity/audit tests | Done: `P11-RESPONSIBILITY-ORPHAN` closed |
| 11D Audience management UI | App Store Sichtbarkeit panel plus badge/drawer shortcuts on launcher/start-menu, Shell tabs and Appbar for preview, Team and restricted audiences with business labels. | 11B/11C | JS UI tests for manager/read-only states, launcher/start-menu badge rendering, data-area labels and accessibility labels | Done: `P11-BADGE-DRAWER-PERMISSIONS` closed with JS tests and Browser/Rust launcher badge plus manager/read-only drawer evidence |

### Phase 12 Slices

| Slice | Goal | Depends on | Minimum local gate | Exit state |
| --- | --- | --- | --- | --- |
| 12A MCP app visibility before data | Evaluate lifecycle/audience visibility for MCP module listing, module detail, links, proposals and execution before data grants. | Phase 11 visibility contract | MCP tests for app-visible/data-denied and app-hidden/data-granted cases | Done: `P12-MCP-APP-VISIBILITY` closed |
| 12B Visible agent scope panels | Add human-readable `Handelt als`, selected app, lifecycle, data areas and external-effect state to every AI-assisted entry point. | 12A | JS/browser tests for global right-click, App Store context chat, Business Chat and coding-agent handoff panels | Done for covered entry points: global right-click `CTOX Zugriff` panel, App Store context-chat panel, Business Chat visible-scope panel and Coding Agent external-scope context are implemented/tested; Browser/Rust proof covers global right-click, App Store context-chat and Business Chat rendered scope |
| 12C Client-context integrity | Ensure submitted `client_context` exactly matches the visible selected app/scope and audit metadata. | 12B | JS/browser tests comparing visible scope to command payload/MCP audit | Done: right-click `visible_scope`, App Store selected-app `visible_scope`, command-bus canonical `scope` normalizer, Business Chat visible-scope rendering and scheduled-context preservation are implemented/tested; Browser/Rust visible-vs-submitted comparison passed for global right-click, App Store context-chat and Business Chat; native/MCP audit metadata parity is closed by `P12-AUDIT-METADATA` |
| 12D Agent grant management | Add business-facing UI/admin surface for agent app visibility and data grants, or document a deliberate admin-only boundary. | 12C | UI tests and MCP policy tests for add/remove agent grants | Done: `P12-AGENT-GRANTS-UI` closed as a read-only Owner/Admin boundary; no UI-only grant mutation without native commands |
| 12E Agent browser smoke | Register and pass `business-os-agent-scope-ui` for denied, read-only and write-capable agent paths. | 12A-12D | Browser/Rust smoke with audit evidence and diagnostics budget | Done for the covered global right-click, App Store context-chat, Business Chat rendered-scope and Settings grant-boundary paths: panel, client-context, hidden app denial, read grant, write denial, audit evidence and diagnostics budget pass |

### Phase 13 Slices

| Slice | Goal | Depends on | Minimum local gate | Exit state |
| --- | --- | --- | --- | --- |
| 13A Inventory and exception policy | Commit packaged/core module and unscoped facade inventory with owner, classification and expiry/review cadence. | Phase 12 complete | Inventory script fails on missing modules/facades | Passed; `P13-MODULE-INVENTORY` closed, and later 13C/13D migration/lint work must update the inventory instead of introducing silent exceptions |
| 13B Real Shell guarded facade | Pass active module into normal `createModuleContext(mod)` -> `createLiveDbFacade(mod)` path and cover collection, property, cached-handle and raw bypasses. | 13A | JS syntax/static checks plus Browser/Rust real-context bypass smoke and persisted runtime `openModule`/reload fixture | Passed; `P13-REAL-SHELL-DB-PATH` closed with real-context and persisted `openModule`/reload guarded-DB evidence |
| 13C Module migration batches | Migrate user-facing packaged modules or add narrow tested system exceptions. | 13A/13B | Module tests, conformance guard, UI regression matrix per batch | Passed for `coding-agents`, `calendar`, `buchhaltung`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `matching`, `notes`, `outbound`, `research`, `shiftflow`, `spreadsheets` and `support`; remaining system/internal modules are closed by 13I scoped exceptions |
| 13D Raw/cached access guard | Extend static conformance to block new raw/unscoped/cached bypass patterns without explicit exception. | 13A/13B | Inventory-drift guard self-test plus `assert-module-conformance.mjs` invoking the DB-isolation guard | Passed; `P13-RAW-DB-LINT` closed for static drift coverage and hardened to 11 self-test cases including optional chaining, dynamic property access and local DB aliases. Module migration itself remains 13C |
| 13E Dynamic app runtime safety boundary | Define trusted-code model for runtime-installed apps, classify allowed fetch/import/storage/external-effect behavior and add negative generated-app fixtures for forbidden capabilities. | 13B/13D | Runtime capability inventory, static guard and Browser/Rust runtime-safety smoke | Passed; `P13-DYNAMIC-RUNTIME-SAFETY` closed with same-origin trusted-code contract, installed-app negative fixtures and Browser/Rust runtime-safety evidence |
| 13F Browser storage scope | Scope browser storage keys where user/workspace relevant and prove storage reset/copy cannot widen roles, visibility, release, audience, tenant or data grants. | 13E/14A | JS/storage static checks plus Dynamic Apps, Audience and Release Browser/Rust storage-boundary smokes | Passed; `P13-BROWSER-STORAGE-SCOPE` closed for scoped UI storage and non-authoritative app visibility/release/audience/data-grant proof. Auth/fresh-profile storage stories remain Phase 14 |
| 13G Guarded-module property/proxy cleanup | Remove remaining inventoried `ctx.db.<collection>` / `ctx.db.collections` fallback paths from guarded packaged/starter modules, or prove each one is a narrow facade-level exception. | 13C/13D | Inventory guard, module conformance, targeted module tests and Dynamic Apps packaged guard smoke | Passed; remaining property/proxy fallbacks were removed from `buchhaltung`, `calendar`, `coding-agents`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `notes`, `outbound`, `shiftflow`, `spreadsheets` and `support`. All 24 module inventory entries now report raw/property/proxy/cached-handle flags as false. Dynamic Apps Browser/Rust smoke passed clean for all 16 packaged guard modules; UI regression also passed functionally with the known Notes Chrome advisory |
| 13H Scoped Shell/Desktop facades | Replace naked `createLiveDbFacade()` calls with scoped system/app facades or narrow reviewed exceptions for Settings, Desktop apps, Business Chat and Reporter companions. | 13A/13D | Inventory guard with fresh facade anchors, targeted desktop/companion tests and UI regression/file-viewer smoke | Passed; Settings, Desktop app windows, Business Chat and Business Reporter now use explicit scoped collection allowlists through `createScopedSystemDbFacade`, inventory reports 0 unscoped facades, targeted Settings/Business Chat/roles/File Viewer smokes pass clean, and UI regression passes functionally with the known Notes Chrome advisory |
| 13I Scoped system/internal module exceptions | Replace raw-free system/internal exception status with exact scoped collection allowlists and Browser/Rust denial proof. | 13A/13C/13G/13H | Inventory guard validates `scoped_collections` against `SCOPED_SYSTEM_MODULE_DB_COLLECTIONS`; Dynamic Apps Browser/Rust smoke proves allowed/foreign/raw/permission/capability contract evidence | Passed; App Store, Browser, Creator, CTOX, Desktop, Knowledge, Reports and Tickets are `system-scoped-exception-tested`/`internal-scoped-exception-tested`, inventory and app.js allowlists match exactly, and `business_os_dynamic_system_scope_*` evidence is accepted warning-clean |

### Phase 14 Slices

| Slice | Goal | Depends on | Minimum local gate | Exit state |
| --- | --- | --- | --- | --- |
| 14A Smoke mode registry | Add release, audience, agent-scope, auth-scope and fresh-profile modes to runner and matrix evidence requirements. | Phases 10-13 smoke needs known | Mode registry/static test and unsupported/missing evidence negative test | Passed; `P14-SMOKE-MODES-REGISTERED` closed. Hosted tenant boundary and scale budgets remain open |
| 14B Auth and protected access stories | Test login, authenticated reload, logout, logged-out reload and blocked protected access with typed config or persisted test-root state. | 14A | Browser/Rust `business-os-auth-scope-ui` | Passed; `P14-AUTH-TENANT-SMOKE` closed for local auth/reload/protected-access and local tenant-scope stability |
| 14C Tenant/workspace boundary | Define local vs hosted tenant claim and prove negative deep-link/data/source/control leakage for the supported boundary. | 14B | Browser/Rust tenant/scope assertions and rollout docs update | Open for hosted/multi-workspace boundary; local tenant-scope stability is covered by 14B |
| 14D Fresh profile and visual label QA | Run empty-storage profile plus desktop/narrow viewport assertions for lifecycle chips, disabled reasons and drawer/detail labels. | 14A-14C | Browser/Rust `business-os-fresh-profile-ui` plus viewport checks | Passed; `P14-VISUAL-LABEL-QA` closed |
| 14E Storage-boundary browser proof | Verify copied/cleared browser storage, reload and second browser context cannot change effective roles, app visibility, audience, release success or data grants. | 13F/14A | Browser/Rust storage-boundary evidence in auth/fresh-profile modes | Passed for local browser storage authority: app visibility, audience, release, data-grant, auth and fresh-profile storage portions are covered. Hosted tenant boundary remains separate in 14C |
| 14F Performance and scale budgets | Add representative fixtures for app catalog size, grants, audit rows and release history, then enforce startup/sync/action duration budgets. | 14A | Browser/Rust scale mode or seeded production smoke with mode-duration/startup/sync budgets | Open |

### Phase 15 Slices

| Slice | Goal | Depends on | Minimum local gate | Exit state |
| --- | --- | --- | --- | --- |
| 15A Why diagnostics | Add operator explanation for actor/app visibility, source, release, rollback and data read/write decisions. | Phases 10-14 behavior stable | Rust diagnostics tests and Settings diagnostics UI tests | Browser/Shell UI slice, native `ctox.business_os.why` command tests, Settings static UI tests, live Settings Browser/Rust proof and support-artifact Why summary passed 2026-06-18; `P15-WHY-DIAGNOSTICS` closed |
| 15B Redacted audit export and retention | Implement export-before-prune, redaction manifest and typed retention config/native state for `business_events`; settle MCP retention boundary. | 15A | Export/redaction/retention Rust tests and rollout docs | MCP retention boundary closed 2026-06-18 via typed `business_os.mcp_policy.v1`; native `ctox.business_os.audit.retention` export-before-prune command slice passed; native `ctox.business_os.audit.retention_policy.set` persists validated retention days as `business_os.audit_retention_policy.v1`; `P15-OPS-RECOVERY` is closed for audit/retention/support evidence. Volume/operations policy remains future work where required |
| 15C Recovery commands and drills | Add tested repair/recovery for stale catalog, orphan private app, bad grant, manifest/release divergence and bad rollback target. | 15A/15B | Rust repair tests and dry-run runbook evidence | Closed: lifecycle projection repair has dry-run, safe projection and apply evidence. `repair_stale_grants` covers the concrete stale module-scoped grant subtype with dry-run/apply tests. `repair_invalid_version_refs` covers broken release snapshot source/rollback version references with dry-run/apply tests. `repair_orphan_private_apps` covers legacy/restore private-app responsibility gaps with dry-run/apply tests and audited App-Verantwortliche assignment. `ctox.module.rollback_version` restores `module.json`, editable source files and removes post-target added files. `P15-REPAIR-COMMANDS` is complete; missing source-version history remains under 15E backup/restore |
| 15D Diagnostics browser smoke | Render diagnostics/Activity with business labels and no redacted content leaks. | 15A-15C | Browser/Rust diagnostics smoke with console/network budgets | Browser diagnostics/support slices passed for 15A/15F; Phase 15 stays `In progress` until 15E backup/restore is closed |
| 15E Backup and restore drill | Test or document restore for runtime store, module manifests, source snapshots, release rows, audit export and retention settings. | 15A-15C | Backup/restore drill and restored-state validation for visibility/grants/rollback | Partial: CLI-only native isolated restore drill passed for core state, CTOX Secret Store, Business OS/native RxDB SQLite stores, installed modules, source snapshots, audit exports, release rows, rollback target, typed MCP policy, typed native audit-retention policy and RxDB catalog projection. The raw snapshot manifest now has Secret-Store-backed HMAC-SHA256 signing, local retention/support-attachment policy, same-version/downgrade compatibility policy and a verified AES-256-GCM portable encrypted export; `ctox business-os backup prune-drills` has dry-run/apply test coverage for expired manifest-retained drill directories. Active-root restore is covered by a machine-readable manual runbook/preflight with quiesce/restart/manifest hash/signature/portable-export/key-escrow gates, not by destructive automation. Browser/Rust `business-os-restore-resync-ui` now covers the local same-profile IndexedDB unsynced-write/WebRTC-peer-restart case. `P15-BACKUP-RESTORE-DRILL` remains open for hosted/multi-workspace WebRTC restore proof, release-level cross-version/downgrade evidence and external key-escrow operational signoff |
| 15F Support artifact schema | Produce a stable redacted diagnostics artifact for support cases and validate schema/redaction before export/prune. | 15A | Artifact schema/redaction tests plus browser diagnostics export smoke | Native schema/redaction command, Settings UI summary/download and Browser/Rust export smoke passed 2026-06-18; `P15-SUPPORT-ARTIFACT-SCHEMA` is closed. Native audit export-before-prune is tracked by 15B/`P15-OPS-RECOVERY` |

### Phase 16 Slices

| Slice | Goal | Depends on | Minimum local gate | Exit state |
| --- | --- | --- | --- | --- |
| 16A Clean checkout JS/bootstrap | Document and automate Business OS JS test dependencies for `esbuild`, Playwright and module tests without local symlinks or host-global packages. | Earlier JS gates known | CI/local bootstrap test on clean checkout | Passed locally and CI-wired 2026-06-18: `P16-JS-BOOTSTRAP` closed |
| 16B Smoke artifact schema | Write fixed-path smoke summary with git revision, URL, role, auth/profile mode, evidence keys, warning budgets and result; validate schema. | Phase 14 modes | Schema validation test and default-path artifact proof; CI upload/retention remains in 16C | Passed locally 2026-06-18: `P16-SMOKE-ARTIFACT` closed for schema/default-path artifact; CI upload/retention remains in `P16-CI-RELEASE-GATES` |
| 16C Required production gate job | Add CI job that runs Rust, RxDB, JS, module conformance and Browser/Rust production smokes, and fails on missing required modes/evidence/skips. | 16A/16B | CI-equivalent local command bundle and mode-registry guard | Passed locally and CI/release-wired 2026-06-18: `P16-CI-RELEASE-GATES` closed with warning-clean six-mode Browser/Rust artifact |
| 16D Release workflow prerequisite | Make tag/release artifact upload depend on the Business OS production gate job. | 16C | Workflow validation/dry-run proof that release cannot upload before gate | Passed locally and release-wired 2026-06-18: `P16-RELEASE-WORKFLOW-GATE` closed |
| 16E Legacy migration fixtures | Commit source-backed private/missing/invalid/released/restricted/preview fixture coverage and migration/backfill tests. | 16C | Migration dry-run and idempotency checks proving no visibility/data widening | Passed locally 2026-06-18: `P16-LEGACY-FIXTURES` closed through the existing Rust lifecycle backfill test |
| 16F Security/privacy signoff | Add a blocking release checklist for dynamic app runtime safety, app source visibility, data-review UX, MCP/agent scopes, audit export, external effects and artifact integrity. | 12-15 complete | Threat-model/security checklist update, redaction proof and artifact integrity proof | In progress 2026-06-18: signoff artifact and release-blocking guard exist; actual signed-off checklist remains open |
| 16G Customer/operator documentation | Publish business-facing docs for roles, lifecycle labels, release/review, preview/restricted sharing, agent scopes, locked data states and recovery. | 16F and current UI evidence | Docs checklist and operator dry-run proving docs match current UI | In progress 2026-06-18: customer guide, operator guide, docs guard and release docs dry-run artifact exist; final human release review/signoff remains open; full production-ready claim can be made only after all phases are `Complete` |

## Phase 0: Baseline and Naming Contract

Goal: create a safe compatibility baseline before behavior changes.

Implementation scope:

- Document current role aliases exactly: `owner -> chef`, `chef`, `admin`,
  `business_os_admin -> admin`, `founder`, `user`,
  `business_os_user -> user`.
- `team -> user` is accepted as an explicit compatibility alias and must remain
  covered by backend and frontend tests.
- Add or update a small compatibility helper for role normalization if needed.
- Keep persisted canonical roles as `chef/admin/founder/user` for this phase.
  Migrating stored role values is a later migration decision because SQLite
  currently enforces these values with a role CHECK.
- Define user-facing labels and descriptions in one shared UI helper.
- Keep all existing behavior unchanged.
- Add a bootstrap/actor-identity note for command authorization: actor id comes
  from command context, but role must be resolved from persisted
  `business_users`; client-supplied role remains non-authoritative. This is not
  yet a complete identity-auth model because bootstrap/local fallback behavior
  still exists when no persisted users are present and login is not required.

Acceptance criteria:

- Existing `chef/admin/founder/user` data remains valid.
- UI copy can show `Owner`, `Admin`, `App-Verantwortliche:r`, `Teammitglied`
  without changing backend authorization.
- `team` and `business_os_team` are explicitly normalized to `user` in Rust and
  JavaScript with tests.
- The plan file is updated with the chosen canonical-role and `team` alias
  decision.

Required tests:

- Existing Rust Business OS tests pass.
- Existing browser/RxDB Business OS tests pass.
- Add focused unit tests for role normalization aliases if helper code changes.
- Add frontend role-label tests if UI helpers move or labels change.

Suggested commands:

```sh
cargo test --bin ctox business_os
node src/apps/business-os/modules/app-store/app-store.test.mjs
node src/apps/business-os/rxdb/tests/run-all.mjs
```

Phase evidence:

- Status: Complete
- Evidence links:
  - PASS `node src/apps/business-os/shared/roles.test.mjs` — 4 role alias,
    label, description and manageability tests passed.
  - PASS `cargo test --bin ctox business_role_normalization_preserves_phase0_aliases`
    — backend role aliases passed.
  - PASS `cargo test --bin ctox rxdb_command_auth_uses_trusted_user_role_not_client_claims`
    — client role spoofing remains rejected.
  - PASS `node --check src/apps/business-os/app.js`
  - PASS `node --check src/apps/business-os/shared/react-settings.js`
  - PASS `node --check src/apps/business-os/shared/roles.js`
  - PASS `node src/apps/business-os/rxdb/tests/run-all.mjs` — 37 passed,
    0 failed, 2 skipped because the wire daemon is not built.
  - PASS `cargo test --bin ctox direct_module_catalog_projection_includes_installed_modules`
    — installed module catalog projection uses the runtime installed-app root.
  - PASS `cargo test --bin ctox completion_hook_indexes_workspace_outputs_for_business_os`
    — workspace output indexing now queries the native RxDB store path.
  - PASS `cargo test --bin ctox business_os` — 196 passed, 0 failed.
  - PASS `node src/apps/business-os/modules/app-store/app-store.test.mjs`
    with `esbuild` installed in `/tmp/ctox-app-store-esbuild` and exposed
    through a temporary module-local `node_modules` symlink that was removed
    after the run — 12 passed, 0 failed.
- Notes:
  - Implemented `team` / `business_os_team` as explicit aliases to canonical
    `user` in Rust and JavaScript.
  - Persisted canonical roles remain `chef/admin/founder/user`.
  - Added shared browser role helper at
    `src/apps/business-os/shared/roles.js`; `app.js` and
    `shared/react-settings.js` now use it for display/normalization.
  - The previous broad Rust gate blockers were stale test assumptions: the
    catalog test now writes the installed manifest under the same runtime
    installed-app root that production loading resolves, and the service test
    now opens `rxdb_store_path(root)` instead of `runtime/ctox.sqlite3`.
  - App Store tests are source-valid but currently require `esbuild` even
    though this repo does not provide a local JS dependency install for
    `src/apps/business-os`. Before CI relies on this gate, either document a
    supported test dependency setup or move the pure App Store test hooks into a
    helper module that can be imported directly without bundling.

## Phase 1: Central Policy Evaluator

Goal: introduce one authoritative policy module without changing the user
experience yet.

Implementation scope:

- Add a central Business OS policy evaluator, for example
  `src/core/business_os/policy.rs`.
- Model roles, permissions, scopes, decisions and display reasons.
- Centralize the existing scattered `chef/admin` and assigned-`founder` gates
  behind the evaluator.
- Keep existing public function names as wrappers where that reduces blast
  radius.
- Add table-driven tests for the default role matrix.
- Preserve the current command/session actor resolution behavior while making
  the trusted source explicit in tests.

Acceptance criteria:

- Current behavior is preserved for module save/delete/release/rollback and user
  management.
- `Owner/Admin` global management and assigned App-Verantwortliche module
  management are tested through the new evaluator.
- Teammitglied cannot modify apps.
- Spoofed client role does not affect a trusted backend decision.
- No UI behavior changes are required in this phase.

Required tests:

- Policy unit tests for each role and major permission.
- Existing spoofed-role tests remain green.
- Existing module-governance tests remain green.
- Existing App Store install/uninstall authorization remains Owner/Admin only.

Suggested commands:

```sh
cargo test --bin ctox rxdb_peer
cargo test --bin ctox business_os
```

After adding the policy module, add and run a focused test filter for the new
policy tests, for example `cargo test --bin ctox business_os_policy`, using the
actual module/test name chosen in implementation.

Phase evidence:

- Status: Complete
- Evidence links:
  - PASS `cargo test --bin ctox business_os_policy` — 3 policy tests passed:
    role alias normalization, default role matrix for major permissions and
    stable in-process denial decision shape.
  - PASS `cargo test --bin ctox business_role_normalization_preserves_phase0_aliases`
    — store role normalization remains compatible with Phase 0 aliases.
  - PASS `cargo test --bin ctox rxdb_command_auth_uses_trusted_user_role_not_client_claims`
    — trusted persisted role still defeats spoofed client role claims.
  - PASS `cargo test --bin ctox business_os` — 199 passed, 0 failed.
  - PASS `node src/apps/business-os/shared/roles.test.mjs` — 5 passed.
  - PASS `node --check src/apps/business-os/shared/roles.js`.
- Notes:
  - Added `src/core/business_os/policy.rs` and exposed it via
    `src/core/business_os/mod.rs`.
  - `normalize_business_role`, `role_can_manage`, `session_can_manage_all` and
    `session_can_modify_module` now route through the policy module while
    preserving existing public wrapper names and current behavior.
  - `upsert_user` now uses the same `session_can_manage_all` policy-backed
    wrapper instead of reading `user.is_admin` directly.
  - Structured denied-command output and app create/modify command gating were
    intentionally deferred from Phase 1 and are now covered by Phase 2
    evidence.
  - Verified gap: current App Store context-menu app modification is still
    gated against the App Store module itself and dispatches
    `record_id: app-store`; selected-app modification must be defined and
    enforced in Phase 2/4 before the UI can claim per-app right-click editing.
  - Verified Phase 1 backend premise: App Store install/uninstall was already
    backend-gated to `chef/admin`; Phase 2 added explicit structured command
    hardening and allow/deny evidence for that family.

## Phase 2: Command Enforcement Hardening

Goal: make backend command acceptance the source of truth for app and admin
actions, including right-click initiated app changes.

Implementation scope:

- Add a command-to-permission mapping for Business OS command types.
- Gate `ctox.business_os.app.modify` and app-create related flows explicitly.
  Current source recognizes them for prompt shaping, then falls through to
  general task creation without a module/role policy gate.
- Gate module manifest save/delete, template install, app install/uninstall,
  release, rollback, user upsert, founder assignment, runtime settings,
  integration/channel changes through the evaluator.
- Leave MCP policy enforcement to Phase 5; Phase 2 is only native
  `business_commands` acceptance and existing control-plane command paths.
- Ensure denied commands write a structured failure state back to
  `business_commands`.
- Preserve WebRTC/RxDB command flow; do not add HTTP write paths.

Acceptance criteria:

- A Teammitglied cannot submit a successful app-modify command by crafting a
  `business_commands` document manually.
- App-create and app-modify commands require explicit policy evaluation before
  queue task creation.
- An App-Verantwortliche:r can modify only assigned modules.
- Owner/Admin can install and uninstall apps.
- Denied commands include at least `permission`, `scope_type`, `scope_id`,
  `reason_code`, `display_reason` and the existing failed command status.
- Non-control `/api/business-os/*` data APIs remain disabled; no new HTTP data
  write path is added.

Required tests:

- Native peer command-consumption tests for allow and deny cases.
- Crafted-command tests that bypass UI visibility and still fail in backend.
- Regression tests for existing module governance commands.
- Denied-command shape test for structured policy fields.
- App create/modify deny tests for Teammitglied and unassigned
  App-Verantwortliche:r.
- App Store install/uninstall allow/deny tests for Owner/Admin vs Teammitglied.

Suggested commands:

```sh
cargo test --bin ctox native_peer_consumes_pending_module_governance_commands
cargo test --bin ctox spoofed
cargo test --bin ctox business_os
node src/apps/business-os/rxdb/tests/run-all.mjs
```

Phase evidence:

- Status: Complete
- Evidence links:
  - PASS `cargo test --bin ctox app_build_commands_enforce_policy_before_queueing`
    — Teammitglied app-modify is denied before queue task creation, spoofed
    client role is ignored, unassigned App-Verantwortliche:r is denied,
    assigned App-Verantwortliche:r may modify the assigned app, Teammitglied
    app-create is denied and Admin app-create is accepted.
  - PASS `cargo test --bin ctox app_create_rxdb_command_accepts_type_alias_from_cli_dispatch`
    — existing app-create CLI/RxDB type alias path still queues correctly for
    an authorized local/admin actor.
  - PASS `cargo test --bin ctox control_commands_return_structured_policy_denials`
    — Teammitglied-crafted control commands are rejected with structured
    `result.policy_decision` for task update/delete, user upsert, subscription
    auth, channel settings, source save/rollback, module
    release/assign/save/delete/template install/rollback/rollback-version and
    App Store install/uninstall.
  - PASS `cargo test --bin ctox native_peer_consumes_pending_ctox_task_update_command`
    — existing authorized CTOX task update command flow still completes after
    the task-manage policy gate was added.
  - PASS `cargo test --bin ctox native_peer_consumes_pending_module_governance_commands`
    — existing native RxDB module-governance allow path remains green.
  - PASS `cargo test --bin ctox app_store_install_uninstall_allows_admin_policy_path`
    — Admin App Store install/uninstall succeeds through a hermetic local ZIP
    server, proving explicit allow behavior without external network downloads.
  - PASS `cargo test --bin ctox business_os_policy` — central evaluator
    regression remains green.
  - PASS `cargo test --bin ctox rxdb_command_auth_uses_trusted_user_role_not_client_claims`
    — trusted persisted role still defeats spoofed client role claims and now
    returns structured `runtime.manage` policy denial data.
  - PASS `cargo test --bin ctox business_os` — 202 passed, 0 failed.
- Notes:
  - Implemented backend gate: generic app create/modify commands are evaluated
    before queued task creation, so UI affordance hiding is no longer the only
    protection for right-click app changes.
  - Backend denial remains authoritative; UI affordance gating in Phase 4 is
    only usability and must not be treated as security.
  - Source decision: structured deny data is stored in the existing
    `business_commands.result.policy_decision` object. The
    `business_commands` schema has `additionalProperties: true`, so this avoids
    schema/hash churn while still making denial fields visible to RxDB clients.
  - Implemented backend catalog slice: user upsert, runtime settings,
    subscription auth, channel commands, task update/delete, source
    save/rollback, module release/assign/save/delete/template
    install/rollback/rollback-version and App Store install/uninstall all route
    through the same structured denial helper.
  - Added explicit App Store admin allow coverage with a local in-process ZIP
    server, so the test does not depend on external download availability.
  - Phase 2 backend scope is ready for review. Lower-risk domain command
    families that are not admin/app-governance actions remain intentionally
    outside this catalog unless a later product policy expands them.
  - IoT widget runtime command handling still keeps its existing management gate
    and hard-error behavior; it was not converted into a `business_commands`
    policy-decision document in this phase.

## Phase 3: Permission Grants and Projection

Goal: support object-scoped and app-scoped permissions without hard-coding every
exception into roles.

Implementation scope:

- Phase 3A architecture decision: use a native, allow-only
  `business_permission_grants` table plus the existing
  `business_module_catalog.governance.permission_model` projection. Do not add
  a new RxDB grant/effective-permission collection in this slice.
- Additive grant storage supports exact scoped rows with
  `subject_type=user|role`, `subject_id`, `permission`, `scope_type`,
  `scope_id`, `active`, timestamps and audit metadata.
- Keep `business_module_acl` as compatibility projection until all callers are
  migrated.
- Project a derived permission model into the existing module catalog governance
  payload. The CTOX module catalog schema allows this without schema/hash
  churn.
- Phase 3A grant enforcement starts module-scoped: backend module decisions
  check role defaults, founder ACL and active exact-scope grants.
- Phase 3B routes selected workspace, task and App Store command families
  through the same grant-aware evaluator. Covered commands include user upsert,
  runtime settings, subscription/channel integration commands, CTOX task
  update/delete, app create/install style gates, App Store install/uninstall,
  template install and module founder assignment.
- Include grants for user, role, workspace, module and task scopes first;
  collection, record, approval and MCP scopes remain representable vocabulary
  but not fully enforced everywhere.
- Add deny support only if product semantics require it; if added, deny wins
  over allow and must be tested. Phase 3A keeps `deny_supported=false`.
- If a later slice adds new core/shared RxDB collections, update exact schema
  artifacts: module/source schema, generated
  `business_os_schema_contract.json`, `business_os_schema_hashes.json`, browser
  hash registry in `src/apps/business-os/rxdb/src/schema.mjs`, and
  dist/cache-busters only through the supported source rebuild path.
- Update WebRTC protocol fixtures only if the transport/wire protocol changes.

Acceptance criteria:

- Effective module permissions can be computed for a user and module without
  adding a new role.
- Existing Founder/App-Verantwortliche assignments still work.
- A future special grant, such as `external.approve` for one module, can be
  represented without a new role.
- Schema contracts remain unchanged when using the existing catalog projection;
  if a later collection is added, contracts are regenerated and guard suites
  remain green.
- Existing `business_module_acl` rows continue to project and authorize assigned
  App-Verantwortliche modules during migration.
- Exact workspace and task grants can authorize the covered native command
  families without creating a new global role.

Required tests:

- Rust store migration tests for new tables/columns.
- RxDB schema/hash tests if collections change; for Phase 3A, run hash smoke to
  prove the projection path stayed schema-stable.
- Browser projection tests for effective permissions once Phase 4 consumes the
  projection.
- Backward compatibility tests for old module ACL rows.
- Native projection tests for grants and the existing catalog
  `permission_model` projection.
- Deny-over-allow tests only if deny semantics are introduced.

Suggested commands:

```sh
cargo test --bin ctox business_os
cargo test --manifest-path src/core/rxdb/Cargo.toml
node src/apps/business-os/rxdb/tests/schema-hash-registry-smoke.mjs
node src/apps/business-os/rxdb/tests/contract-drift-smoke.mjs
node src/apps/business-os/rxdb/tests/run-all.mjs
```

Phase evidence:

- Status: Complete
- Evidence links:
  - PASS `cargo test --bin ctox permission_grant_allows_scoped_module_action_without_new_role`
    — a Teammitglied with an explicit `apps.modify` module grant can modify
    exactly that module and is still denied on another module.
  - PASS `cargo test --bin ctox module_governance_projection_includes_permission_model_and_grants`
    — module catalog governance projects role defaults, founder assignment
    permissions and active explicit grants.
  - PASS `cargo test --bin ctox business_os` — 207 passed, 0 failed.
  - PASS `node src/apps/business-os/rxdb/tests/schema-hash-registry-smoke.mjs`
    — browser/runtime schema hash registry remains unchanged and valid.
  - PASS `node src/apps/business-os/rxdb/tests/run-all.mjs` — 37 passed,
    0 failed, 2 skipped because the wire daemon is not built.
  - PASS `node src/apps/business-os/shared/roles.test.mjs` — 5 passed,
    including Owner-only assignable role options.
  - PASS `node src/apps/business-os/scripts/assert-module-conformance.mjs`
    — 23 modules conform.
  - PASS `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
    — Business OS browser data path remains RxDB-only.
  - PASS `cargo test --bin ctox workspace_permission_grant_allows_user_upsert_without_admin_role`
    — an exact workspace `users.manage` grant allows a Teammitglied to execute
    the user-upsert command while spoofed client admin claims remain
    non-authoritative.
  - PASS 2026-06-17 `cargo test --bin ctox owner_role` — Admins and exact
    workspace `users.manage` grants cannot assign canonical Owner
    (`chef`/`owner`) through `ctox.business_os.user.upsert`, while an Owner can.
  - PASS 2026-06-17 `cargo test --bin ctox native_peer_consumes_pending_module_governance_commands`
    — native RxDB command consumption now proves the same boundary: Admin
    owner-upsert is denied with `workspace.manage`, while a persisted Owner can
    assign canonical Owner.
  - PASS `cargo test --bin ctox task_permission_grant_allows_ctox_task_update_without_admin_role`
    — an exact task `ctox.task.manage` grant allows a Teammitglied to update
    that queue task without a global admin role.
  - PASS 2026-06-17 Phase 3C source validation — `ctox.task.manage` has
    evaluator hooks for owned/assigned scopes, but current native and MCP task
    policy construction passes both ownership flags as false. `QueueTaskView`
    exposes `lease_owner`, but source shows it is routing runtime state, not a
    Business OS owner or assignee field.
  - PASS 2026-06-17 `cargo test --bin ctox task_permission_grant_allows_ctox_task_update_without_admin_role`
    — exact task grants remain the implemented non-admin task-management path.
  - PASS 2026-06-17 `cargo test --bin ctox record_owner_payload_field_does_not_grant_native_policy_access`
    — owner-like Record payload fields such as `owner_id` do not become native
    implicit `data.read` record grants.
  - PASS 2026-06-17 `cargo test --bin ctox source_load_requires_source_view_permission`
    — Teammitglied source load and source snapshot listing are denied with
    `apps.source.view`; an exact module source-view grant allows both while
    spoofed client admin claims remain non-authoritative.
  - PASS `cargo test --bin ctox app_store_install_uninstall_allows_explicit_module_grants`
    — exact module `apps.install` and `apps.uninstall` grants authorize App
    Store install/uninstall through the same hermetic local ZIP path as the
    Admin allow test.
  - PASS `cargo test --bin ctox control_commands_return_structured_policy_denials`
    — existing structured denial behavior remains intact after grant-aware
    routing.
  - PASS `cargo test --manifest-path src/core/rxdb/Cargo.toml` — 239 unit
    tests and 30 conformance tests passed.
  - PASS `cargo check`.
- Notes:
  - Phase 3A implements the low-risk path confirmed by read-only backend and
    RxDB subagents: native table plus existing catalog projection, no new RxDB
    collection and no dist/cache-buster changes.
  - Phase 3B implemented: selected workspace-, task- and App Store command
    decisions plus their inner side-effect guards now use the same grant-aware
    evaluator, so exact scoped grants are enforced beyond module modification.
  - Phase 3E implemented source read gates: `ctox.source.load` and
    `ctox.source.list_snapshots` use grant-aware `apps.source.view`, while
    `ctox.source.save` remains gated by `apps.modify` and rollback by
    `apps.rollback`.
  - Current browser logic still reads roles and module governance directly.
    Phase 4 must consume the projected permission model through one shared
    browser helper instead of duplicating role matrices.
  - Explicit grants are still not exhaustive across every backend action.
    Covered backend scopes are module, selected workspace command families,
    CTOX task update/delete and App Store install/uninstall. Later Phase 5
    work covers MCP collection, exact record and exact approval scopes. Native
    non-MCP record ownership derivation and task ownership derivation remain
    separate product slices if policy requires them. Phase 3F accepts the
    current rollout boundary: object ownership is not inferred from arbitrary
    record payload fields until a normalized ownership model exists.

## Phase 4: Business-Facing UI

Goal: expose the model as a clear Business OS experience instead of an
IT-heavy permission editor.

Implementation scope:

- Rename UI concepts:
  `Chef` -> `Owner`, `Founder` -> `App-Verantwortliche:r`,
  `User` -> `Teammitglied`.
- Replace `User Management` with `Team & Zugaenge`.
- Rename and reshape the existing Founder assignment surface backed by
  `business_module_acl` / `ctox.module.assign_founder`; do not create a second
  assignment model.
- Build right-click menus from projected effective permissions once Phase 3
  provides them. Until then, keep the existing role/governance checks but avoid
  introducing new duplicated matrices.
- Replace duplicated browser permission checks in the shell, Settings and App
  Store with one shared permission helper. Current duplicates include local
  `chef/admin/founder` checks, `governance.founders` checks and separate role
  normalization code.
- Show disabled high-value actions with a plain reason where hiding would be
  confusing.
- Gate App Store install, uninstall, edit and modify affordances by role in the
  UI; backend authorization remains authoritative.
- Define selected-app semantics for App Store right-click app modification.
  Current source can modify the App Store module itself, not necessarily the
  selected installed app.
- Keep normal app usage uncluttered for Teammitglieder.
- Add a lightweight audit/decision view only after Phase 2 provides structured
  denial fields and Phase 6 provides a Business OS policy-decision event source.

Acceptance criteria:

- Teammitglied sees no app-modification action for an app.
- App-Verantwortliche:r sees `App aendern` only on assigned apps.
- Owner/Admin see app install, uninstall, assign responsible and admin settings.
- UI labels do not expose `chef`, `founder`, raw capability names or internal
  reason codes.
- UI labels no longer expose `Chef`, `Founder`, `User Management`,
  `Founder Review` or `Founder zuweisen` in the redesigned settings surface.
- Denial messages are understandable without technical knowledge.
- App Store controls are consistent with backend policy for Owner/Admin,
  assigned App-Verantwortliche:r and Teammitglied.

Required tests:

- JS unit tests for permission-driven menu rendering.
- Role label/alias tests for `Owner`, `App-Verantwortliche:r` and
  `Teammitglied`.
- Static guard that rejects new local role matrices and raw
  `governance.founders` permission checks outside the shared permission helper.
- Settings rendering tests proving old labels no longer leak.
- Right-click tests for Owner/Admin, assigned App-Verantwortliche:r, unassigned
  App-Verantwortliche:r and Teammitglied.
- App Store tests for install/uninstall/edit visibility by role.
- Test that App Store app-modify targets the selected app when that behavior is
  implemented.
- Command refresh test proving a changed module assignment or grant updates UI
  affordances after the catalog projection refreshes, without a page reload.
- Browser smoke or Playwright test for readable denial messages and disabled
  action reasons.

Suggested commands:

```sh
node src/apps/business-os/modules/app-store/app-store.test.mjs
node src/apps/business-os/rxdb/tests/command-bus-projection-smoke.mjs
node src/apps/business-os/rxdb/tests/run-all.mjs
```

Phase evidence:

- Status: Complete
- Evidence links:
  - PASS `node --check src/apps/business-os/shared/permissions.js && node --check src/apps/business-os/shared/permissions.test.mjs && node --check src/apps/business-os/modules/app-store/index.js && node --check src/apps/business-os/modules/app-store/app-store.test.mjs && node --check src/apps/business-os/shared/react-settings.js && node --check src/apps/business-os/app.js`.
  - PASS `node src/apps/business-os/shared/permissions.test.mjs` — 4 passed:
    actor role aliases, assigned-module permissions, explicit projected grants
    including inactive-grant ignore, and exact workspace grants.
  - PASS `node src/apps/business-os/shared/roles.test.mjs` — 5 passed,
    including Owner-only assignable role options.
  - PASS `node src/apps/business-os/modules/app-store/app-store.test.mjs` with
    `esbuild` resolved through a temporary module-local `node_modules` symlink
    to `/tmp/ctox-app-store-esbuild/node_modules`; the symlink was removed after
    the run — 15 passed, 0 failed.
  - PASS `node src/apps/business-os/shared/react-settings.test.mjs` — 4 passed:
    Settings user and module tabs render business-facing labels, Admin no
    longer sees the Owner target-role option, and old visible `User Management`,
    `Founder Review`, `Founder:` or `Founder zuweisen` wording is absent.
  - PASS `node src/apps/business-os/scripts/assert-permissions-ui.mjs` —
    static guard rejects new local owner/admin role arrays, founder permission
    branches, raw founder permission checks and old Settings labels in
    Shell/Settings/App Store, plus old app-modification wording such as
    `App modifizieren` and `Modul bearbeiten`.
  - PASS `node src/apps/business-os/shared/shell-permissions-ui.test.mjs` —
    2 passed: shell module right-click menu renders `App ändern` for Owner,
    Admin, assigned App-Verantwortliche:r and explicit module grants, while
    hiding it for unassigned App-Verantwortliche:r and Teammitglieder; global
    CTOX context modes expose app-change mode only when modification is
    permitted.
  - PASS `node src/apps/business-os/scripts/assert-module-conformance.mjs` —
    23 modules conform.
  - PASS `node src/apps/business-os/scripts/assert-rxdb-only.mjs` — RxDB-only
    contract OK.
  - PASS 2026-06-17 `node src/apps/business-os/shared/shell-permissions-ui.test.mjs`
    — routine Shell `App ändern` affordances are hidden when app modification
    is not permitted, and Shell/Appbar Source affordances follow
    `apps.source.view`.
  - PASS 2026-06-17 `node src/apps/business-os/desktop-apps/code-editor/code-editor.test.mjs`
    via temporary module-local `node_modules` symlink to
    `/tmp/ctox-app-store-esbuild/node_modules`; the symlink was removed after
    the run — 8 passed, covering Source Editor source-view filtering and
    failed source-command denial handling.
  - PASS 2026-06-17 `node src/apps/business-os/modules/app-store/app-store.test.mjs`
    via temporary module-local `node_modules` symlink to
    `/tmp/ctox-app-store-esbuild/node_modules`; the symlink was removed after
    the run — 15 passed, covering disabled App Store reasons and recompute
    after grants refresh.
  - PASS 2026-06-17 `node src/apps/business-os/scripts/assert-permissions-ui.mjs`
    — static UI guard remains green for business-facing wording.
  - PASS 2026-06-17 read-only subagent Galileo — Deny UI mixed pattern is
    source-true; remaining item is documentation status only.
  - PASS `node src/apps/business-os/rxdb/tests/run-all.mjs` — 37 passed,
    0 failed, 2 skipped because the wire daemon is not built.
  - PASS static Playwright smoke against
    `http://127.0.0.1:8765/business-os/index.html#app-store` with URL-packed
    pairing config: App Store rendered with title
    `App Store · CTOX Business OS (BROWSE)`, account badge rendered
    `Owner Smoke@BROWSE`, App Store controls were visible, and the visible
    page text did not contain old labels `User Management`, `Founder Review`,
    `Founder zuweisen` or `Chef`. Console errors were limited to the expected
    WebSocket signaling failure because the smoke intentionally did not start a
    native RxDB/WebRTC peer.
  - PASS static Playwright Teammitglied smoke against
    `http://127.0.0.1:8765/business-os/index.html#app-store` with URL-packed
    pairing config: App Store rendered with title
    `App Store · CTOX Business OS (BROWSE)`, account badge rendered
    `Team Smoke@BROWSE`, unavailable install actions rendered disabled with
    `data-disabled-reason`, and the visible page text did not contain old
    labels `User Management`, `Founder Review`, `Founder zuweisen`, `Chef`,
    `App modifizieren` or `Modul bearbeiten`. Console errors were limited to
    the expected WebSocket signaling failure because the smoke intentionally
    did not start a native RxDB/WebRTC peer.
- Notes:
  - Phase 4A implemented a shared browser permission helper at
    `src/apps/business-os/shared/permissions.js`. It consumes
    `governance.permission_model.role_defaults`, `module_assignments` and
    `explicit_grants`, while keeping a compatibility fallback for existing
    `governance.founders`.
  - Shell right-click/app visibility checks, Settings module-admin filtering and
    App Store install/update/edit/uninstall/context-modify affordances now route
    through the shared helper instead of local role matrices.
  - Settings role options are now actor-aware: Owner can assign Owner, while
    Admin can assign Teammitglied, App-Verantwortliche:r and Admin but not
    Owner. The backend `workspace.manage` gate remains authoritative for
    manipulated commands.
  - App Store context app-modify now targets the selected app via
    `record_id`, `payload.module_id`, `payload.app_id` and
    `client_context.module_id/app_id` instead of only targeting `app-store`.
  - Settings visible labels in the touched Team/Module surfaces now use
    `Team & Zugaenge`, `Teammitglied`, `Owner`, `App-Verantwortliche:r` and
    `Verantwortliche:n zuweisen`; storage values remain
    `chef/admin/founder/user`.
  - Phase 4B added a static UI permission guard and Settings render-label tests;
    the role badge now renders `Owner`/business-facing labels instead of raw
    stored values such as `chef`.
  - Phase 4C implemented disabled App Store high-value actions with
    plain-language reasons for relevant install/update/uninstall actions when
    metadata says the action exists but projected permissions deny it. App
    modification/edit remains hidden for unauthorized users to keep normal Team
    usage uncluttered.
  - Phase 4C added an App Store affordance-refresh hook test proving the same
    card actions recompute from denied/disabled to active after a refreshed
    governance projection adds exact grants, without page-reload-only logic.
  - Phase 4D added a pure shell-permission UI helper for module right-click
    items and global CTOX context modes. The visible action wording is now
    `App ändern`; old `App modifizieren` / `Modul bearbeiten` wording is
    guarded against in the Shell/Settings/App Store permission surfaces.
  - Phase 4F closes Source UI visibility: Shell right-click Source, module
    Appbar Source, direct `openModuleSourceEditor` and the Source Editor module
    picker now consume the projected `apps.source.view` model. The Source
    Editor treats failed source commands as hard errors and switches to
    read-only unless `apps.modify` is also allowed.
  - Phase 4 scope is ready for review. Full role-by-role manual UX smoke stays
    useful for rollout, but the planned automated Phase 4 coverage is present.

## Phase 5: MCP and External-Agent Alignment

Goal: make MCP, browser UI and native command validation use the same authority
model.

Implementation scope:

- Map MCP actor identities to Business OS actors or explicit service actors.
- Introduce the shared evaluator into MCP. Current MCP uses its own policy
  allowlists and runtime operator config, not the Business OS role evaluator.
- Evaluate MCP reads, writes and reachable approval paths through the shared
  evaluator once actor mapping is defined; external-effect execution remains
  blocked by MCP Channel v1 guards unless a later phase deliberately opens it.
- Keep MCP allowlists as additional constraints, not a second independent role
  system.
- Preserve existing `business_os_mcp_events` audit, then add compatible
  policy-decision fields if needed.
- Keep external effects blocked for MCP Channel v1/current rollout. Current MCP
  blocks external effects even when approved; enabling approved external
  effects is new product behavior, not current source behavior.

Acceptance criteria:

- An MCP actor cannot read or modify modules outside its allowed Business OS
  scope.
- MCP denied decisions and browser denied decisions use compatible reason codes.
- Existing MCP policy allowlists still work as stricter filters.
- External effects remain blocked for the current rollout; approval-gated
  execution requires a later product decision and test coverage.
- MCP audit remains queryable after policy integration.

Required tests:

- MCP policy tests for allowed and denied actors/modules/collections.
- Approval-required tests for external-effect tools.
- Regression tests for existing MCP channel status/read tools.
- Actor-to-`business_users` mapping tests.
- MCP audit tests that include policy decision/reason fields if added.
- External-effect tests covering the current blocked behavior and any future
  approved-execution behavior.

Suggested commands:

```sh
cargo test --bin ctox mcp_channel
cargo test --bin ctox business_os
```

Phase evidence:

- Status: Complete
- Evidence links:
  - PASS `cargo test --bin ctox mcp_business_os_policy` — 14 passed:
    ungranted Teammitglied MCP action execution is denied with
    `business_os_policy`, an explicit module `data.write` grant allows a
    Teammitglied delegate to execute `customers.create_followup`, ungranted
    Teammitglied module detail reads are denied, and an explicit module
    `data.read` grant allows collection reads through the module catalog
    collection mapping. Phase 5C adds coverage for module-list filtering by
    module `data.read`, MCP status allow/deny by `mcp.manage`, scoped
    `open_link` denial without module read access, and command-status denial
    without `business_commands` read access. Phase 5D adds coverage that an
    unmapped MCP actor does not inherit local bootstrap Admin rights, and that
    an unpersisted service actor can execute only when an exact
    `subject_type=user` grant matches its actor id. Phase 5E adds coverage that
    an exact record grant allows only `business_os.get_record` for that record
    and does not allow collection listing, and that exact approval grant
    routing exists for the targeted approval decision path. `business_os.approve`
    is still classified as an MCP external-effect tool and is blocked by the
    outer MCP policy by default.
  - PASS `cargo test --bin ctox audited_mcp_policy_denial_records_business_os_policy_decision`
    — denied MCP action execution records a failed `business_os_mcp_events`
    entry with `metadata.policy_decision` containing `allowed=false`,
    `permission=data.write`, `scope_type=module`, `scope_id=customers` and
    `reason_code=role_or_scope_denied`.
  - PASS `cargo test --bin ctox audited_mcp_read_denial_records_business_os_policy_decision`
    — denied MCP collection read records a failed `business_os_mcp_events`
    entry with `metadata.policy_decision` containing `allowed=false`,
    `permission=data.read`, `scope_type=collection`,
    `scope_id=customer_accounts` and `reason_code=role_or_scope_denied`.
  - PASS `cargo test --bin ctox execute_action_blocks_external_effects_even_when_approved`
    — existing MCP v1 external-effect block remains in force after the actor is
    allowed by Business OS policy.
  - PASS `cargo test --bin ctox approval_decision_enqueues_typed_outbound_command`
    — direct approved outbound approval command still enqueues for an Admin
    actor after the new `external.approve` gate.
  - PASS `cargo test --bin ctox request_changes_enqueues_typed_outbound_command`
    — direct request-changes approval command still enqueues for an Admin actor
    after the new `external.approve` gate.
  - PASS `cargo test --quiet --bin ctox business_os::mcp_channel::tests` —
    53 passed, 0 failed.
  - PASS `cargo test --quiet --bin ctox business_os::policy` — 3 passed,
    0 failed.
  - PASS 2026-06-17 source validation for Phase 5F decision boundaries:
    `execute_action` checks policy and confirmation before the explicit
    `ExternalEffectBlocked` branch, exact record and approval grants are routed
    by shared policy, and `trusted_actor_policy_decision_with_conn` only derives
    founder module ownership automatically. Task, record and approval scopes do
    not derive arbitrary object ownership in this path.
  - PASS 2026-06-17 source validation for MCP approval-tool classification:
    `business_os.approve` is `McpToolPolicyClass::ExternalEffect`, while
    `business_os.reject` and `business_os.request_changes` are
    `McpToolPolicyClass::Approval`. The shared `external.approve` decision path
    exists, but the outer external-effect policy blocks `business_os.approve` by
    default before that path is reachable through `call_tool`.
  - PASS 2026-06-17 source validation for Phase 5G MCP external-effect
    boundary: `call_tool` enforces outer MCP tool policy before dispatch,
    `mcp_policy` defaults `allow_external_effects=false`,
    `business_os.approve` is classified as `ExternalEffect`, and
    `execute_action` still returns `ExternalEffectBlocked` after shared policy
    and confirmation when the proposed action has `external_effect=true`.
  - PASS 2026-06-17 `cargo test --bin ctox execute_action_blocks_external_effects_even_when_approved`
    — 1 passed, 0 failed.
  - PASS 2026-06-17 `cargo test --bin ctox mcp_policy_blocks_external_effect_approval_by_default`
    — 1 passed, 0 failed.
  - PASS 2026-06-17 `cargo test --bin ctox mcp_business_os_policy_denies_other_approval_for_exact_approval_grant`
    — exact approval grants remain approval-id scoped.
  - PASS 2026-06-17 `cargo test --bin ctox mcp_approval_actor_user_id_does_not_replace_exact_approval_grant`
    — `outbound_approvals.actor_user_id` is not treated as a pre-decision
    approval owner or implicit `external.approve` grant.
  - PASS 2026-06-17 `cargo test --bin ctox mcp_record_owner_payload_field_does_not_replace_exact_record_grant`
    — owner-like Record payload fields do not replace exact MCP
    `scope_type=record` grants for `business_os.get_record`.
  - PASS 2026-06-17 `cargo test --bin ctox mcp_business_os_policy` — 14
    passed, 0 failed.
  - PASS 2026-06-17 `cargo test --quiet --bin ctox business_os::policy` — 3
    passed, 0 failed.
- Notes:
  - Implemented a strict trusted MCP actor policy bridge in
    `store::trusted_mcp_actor_policy_decision` and
    `store::trusted_mcp_actor`; it resolves the MCP actor against persisted
    `business_users`, otherwise treats the raw MCP actor id as a synthetic
    `user` service actor for exact `subject_type=user` grants. This keeps
    service accounts on the existing `business_users`/grant model and avoids a
    new technical role or grant subject type.
  - Unmapped MCP actors no longer inherit the local desktop bootstrap Admin
    session. Existing gateway/MCP actor allowlists still use the raw MCP actor
    string and remain stricter filters before shared Business OS policy.
  - `business_os.execute_action` now requires shared `data.write` on the target
    module before recording a command. `business_os.approve`,
    `business_os.reject` and `business_os.request_changes` now require shared
    `external.approve` scoped to the exact approval when `approval_id` is
    present, with fallback to the existing `outbound` module approval grant.
  - Module-detail/action-proposal reads now require shared `data.read` on the
    target module. Collection and aggregate record reads now require shared
    `data.read` on the exact collection or a module-level `data.read` grant via
    catalog collection mapping. Exact `business_os.get_record` reads first
    check `scope_type=record` with `scope_id=<collection>/<record_id>` and only
    fall back to the collection/module read path when no exact record grant
    allows the request.
  - `business_os.list_modules` now filters module descriptors through the old
    MCP module allowlist plus shared module `data.read`/`data.write`
    visibility. `business_os.open_link` now checks shared `data.read` for the
    linked module or collection, `business_os.get_command_status` checks
    shared `data.read` on `business_commands`, and `business_os.status` plus
    `business_os.list_mcp_activity` check shared `mcp.manage` on the MCP
    scope.
  - `business_os.execute_action` now checks shared `data.write` before the
    proposal lookup, so an ungranted actor gets a policy denial instead of
    leaking module existence through a proposal-time not-found path.
  - MCP action proposal/execution command contexts now write
    `client_context.actor` as an object with `id`, `display_name`, `role`,
    `persisted` and `raw_actor`, plus `client_context.mcp_actor` for the raw
    MCP identity. This keeps downstream native command attribution on the
    resolved Business OS actor instead of falling back to `rxdb-command`.
  - MCP audit metadata now includes a compatible `policy_decision` object for
    shared-policy-covered tools on success and failure, plus
    `metadata.resolved_actor` for the Business OS actor used by policy.
  - Phase 5F is a source-validation slice, not a behavior change. It confirms
    that the remaining Phase 5 work needs product semantics before code changes:
    exact record grants use `scope_id=<collection>/<record_id>`, exact approval
    decision routing uses `scope_type=approval`, and current MCP external
    effects stay blocked even after shared policy and confirmation succeed.
    Euler found one residual exact-approval negative test gap; Phase 5F closed
    it with `mcp_business_os_policy_denies_other_approval_for_exact_approval_grant`.
  - Phase 5G accepts the current MCP Channel v1 external-effect boundary for
    this rollout: external MCP effects stay blocked by default and
    `execute_action` keeps its explicit `ExternalEffectBlocked` guard. Enabling
    approved external effects is deferred to a later product phase with its own
    risk model and tests.
  - Phase 5H accepts the exact-grants-only ownership boundary for this rollout:
    automatic record/approval ownership derivation is deferred until a
    normalized ownership contract defines authoritative owner/assignee fields
    and read/write/approval semantics. Many-to-one external actor aliasing,
    service-account lifecycle metadata and any future approval-gated
    external-effect execution remain later product metadata/behavior, not
    current rollout blockers.

## Phase 6: Migration, Audit, Rollout

Goal: make the new permission model safe to ship and supportable in existing
CTOX instances.

Implementation scope:

- Complete rollout and compatibility work that was not already required inside
  earlier phases. Schema/migration work required by Phase 3 must happen inside
  Phase 3, not be deferred here.
- Add migration for old role labels and module ACL rows if canonical storage
  changes. If storage remains `chef/admin/founder/user`, document why no role
  value migration is needed.
- Add or finalize audit/event storage for important allow/deny decisions.
  Current source has `business_events` and MCP-specific
  `business_os_mcp_events`. Phase 6A reuses `business_events` for native
  denied policy decisions, Phase 6B adds role/app-responsibility changes, and
  Phase 6C exposes those native events through an admin-only Activity command
  and Settings tab. Phase 6D adds external approval activity by recording
  native Outbound approval decisions in the same native event stream. Phase 6F
  records allowed policy decisions from existing native PolicyDecision gates in
  the same stream as `business_os.policy.allowed`; the Activity-list command is
  intentionally excluded from allowed self-logging.
- Add admin-visible activity for role changes, app-responsible changes, denied
  and allowed native policy decisions and external approval decisions.
- Add release notes and operator guidance.
- Define rollback behavior for partially migrated instances.
- Run full guard suites before marking complete.

Acceptance criteria:

- Existing instances with `chef/admin/founder/user` continue to start and sync.
- No existing owner/admin loses access during migration.
- Permission events are queryable for support/debugging.
- Rollout docs explain the new role labels and their old aliases.
- Partial migrations have a documented rollback or recovery path.
- MCP audit and general Business OS policy audit are clearly distinguished or
  intentionally unified.

Required tests:

- Migration tests on representative old stores.
- Full Business OS Rust tests.
- Full RxDB browser tests.
- Manual smoke test for each role.
- Audit query tests for denied app-modify and role-change events if a new audit
  store is added.
- Migration rollback/recovery tests if stored role values or ACL schema change.

Suggested commands:

```sh
cargo test --bin ctox business_os
cargo test --manifest-path src/core/rxdb/Cargo.toml
node src/apps/business-os/rxdb/tests/run-all.mjs
```

Phase evidence:

- Status: Complete
- Evidence links:
  - PASS 2026-06-17: `cargo test --bin ctox policy_denial_writes_business_event_audit`
  - PASS 2026-06-17: `cargo test --bin ctox control_commands_return_structured_policy_denials`
  - PASS 2026-06-17: `cargo test --bin ctox app_build_commands_enforce_policy_before_queueing`
  - PASS 2026-06-17: `cargo test --quiet --bin ctox business_os::policy`
  - PASS 2026-06-17: `rustfmt --edition 2021 --check src/core/business_os/store.rs src/core/business_os/mcp_channel.rs`
  - PASS 2026-06-17: `git diff --check -- src/core/business_os/store.rs src/core/business_os/mcp_channel.rs`
  - PASS 2026-06-17: `! grep -n '[[:blank:]]$' docs/business-os-roles-permissions-plan.md`
  - PASS 2026-06-17: `cargo test --bin ctox user_role_change_writes_business_event_audit`
  - PASS 2026-06-17: `cargo test --bin ctox module_founder_assignment_writes_business_event_audit`
  - PASS 2026-06-17: `cargo test --bin ctox workspace_permission_grant_allows_user_upsert_without_admin_role`
  - PASS 2026-06-17: `cargo test --bin ctox native_peer_consumes_pending_module_governance_commands`
    — native peer governance commands align with the Owner-only transfer
    boundary: Admin owner-upsert fails; persisted Owner owner-upsert succeeds.
  - PASS 2026-06-17: `cargo test --bin ctox audit_list_command`
  - PASS 2026-06-17: `cargo test --bin ctox business_event_audit`
  - PASS 2026-06-17: `cargo test --bin ctox outbound_approval_decisions_write_business_event_audit`
  - PASS 2026-06-17: `node src/apps/business-os/shared/react-settings.test.mjs`
  - PASS 2026-06-17: `node --check src/apps/business-os/shared/react-settings.js`
  - PASS 2026-06-17: `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
  - PASS 2026-06-17: `rustfmt --edition 2021 --check src/core/business_os/store.rs`
  - PASS 2026-06-17: `git diff --check -- src/core/business_os/store.rs src/apps/business-os/shared/react-settings.js src/apps/business-os/shared/react-settings.test.mjs docs/business-os-roles-permissions-plan.md`
  - PASS 2026-06-17: `test -s docs/business-os-roles-permissions-rollout.md`
  - PASS 2026-06-17: `rg -n "Business OS Roles and Permissions Rollout Guide|Recovery|Ship Criteria" docs/business-os-roles-permissions-rollout.md`
  - PASS 2026-06-17: `cargo test --bin ctox allowed_policy_decision_writes_business_event_audit`
  - PASS 2026-06-17: `cargo test --bin ctox business_event_audit` — 5 audit tests passed.
  - PASS 2026-06-17: `cargo test --bin ctox audit_list_command`
  - PASS 2026-06-17: `node src/apps/business-os/shared/react-settings.test.mjs`
  - PASS 2026-06-17: `node --check src/apps/business-os/shared/react-settings.js`
  - PASS 2026-06-17: `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
  - PASS 2026-06-17: `rustfmt --edition 2021 --check src/core/business_os/store.rs`
  - PASS 2026-06-17: `git diff --check -- src/core/business_os/store.rs src/apps/business-os/shared/react-settings.js`
  - PASS 2026-06-17: `! grep -n '[[:blank:]]$' docs/business-os-roles-permissions-plan.md docs/business-os-roles-permissions-rollout.md src/apps/business-os/shared/react-settings.test.mjs`
  - PASS 2026-06-17: `cargo test --bin ctox business_os` — 239 tests passed.
  - PASS 2026-06-17: `cargo test --manifest-path src/core/rxdb/Cargo.toml` — 239 unit tests and 30 conformance tests passed.
  - PASS 2026-06-17: `node src/apps/business-os/rxdb/tests/run-all.mjs` — 37 passed, 0 failed, 2 skipped because the wire daemon is not built.
  - PASS 2026-06-17: `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`
    — the required RxDB format guard is green after mechanical formatting of
    previously clean RxDB source files.
  - PASS 2026-06-17: `node src/apps/business-os/shared/roles.test.mjs`
  - PASS 2026-06-17: `node src/apps/business-os/shared/permissions.test.mjs`
  - PASS 2026-06-17: `node src/apps/business-os/shared/shell-permissions-ui.test.mjs`
  - PASS 2026-06-17: `node src/apps/business-os/scripts/assert-permissions-ui.mjs`
  - PASS 2026-06-17: read-only subagents Poincare and Socrates validated Phase 6F and plan coherence; no files edited by subagents.
- Notes:
  - Phase 6A scope is intentionally small and schema-stable: native
    Business-OS policy denials now write a queryable `business_events` audit
    row alongside the existing `business_commands.result.policy_decision`
    payload. No new RxDB collection or browser dist change was introduced for
    this slice.
  - Rollout and recovery guidance now lives in
    `docs/business-os-roles-permissions-rollout.md`.
  - Phase 6B reuses `business_events` for native role and app-responsibility
    changes without introducing a new RxDB collection.
  - Phase 6C uses `ctox.business_os.audit.list` through the existing
    `business_commands` path, gated by `users.manage`, to render role,
    app-responsibility and denied-action activity in Settings. It reads the
    existing native `business_events` table and keeps MCP activity separate in
    `business_os_mcp_events`.
  - Phase 6D records `outbound.message.approve`, `outbound.message.reject` and
    `outbound.message.request_changes` decisions as native `business_events`
    rows and renders them in the same admin Activity tab. The audit payload
    stores decision metadata and selected record snapshots, but not message
    body text.
  - Phase 6F records allowed native policy decisions through the same
    `business_events` payload shape as denials, with
    `event_type=business_os.policy.allowed`. `ctox.business_os.audit.list`
    is intentionally skipped for allowed self-logging, while denied
    Activity-list attempts remain auditable. The event records the policy
    decision, not proof that all later side effects completed.
  - Phase 6G closed the plan-coherence slice after read-only subagent review:
    stale Phase 3/5/6 tracker language was corrected, accepted role-label and
    audit-scope decisions were updated, denied Activity-list audit persistence
    got a direct regression assertion, and the broad Business OS/RxDB release
    gates passed. Future audit retention/volume policy is explicitly not a
    Phase 6 ship blocker.

## Phase 7: Production-Readiness Hardening

Goal: close the gap between implementation-complete and production-ready by
adding a live Browser/Rust role-permission gate and rerunning release evidence
without known skips where the repo provides the required harness.

Implementation scope:

1. Add `business-os-roles-permissions-ui` to the existing full-app
   Browser/Rust smoke harness.
2. Run the role-permission smoke through a fresh persistent browser profile,
   WebRTC/RxDB startup, app reload and the real Shell context-menu/Appbar
   render paths.
3. Validate these user stories in the browser:
   - Teammitglied does not see `App ändern` or `Source öffnen` by default.
   - Exact source-view grant exposes `Source öffnen` and the Appbar source
     action only for the scoped app.
   - Exact modify grant exposes `App ändern` only for the scoped app.
   - Owner sees both app-change and source actions.
   - Owner can assign the Owner target role; Admin cannot.
   - UI labels stay business-facing and avoid legacy raw role wording.
4. Register hard matrix evidence for the new mode so CI fails on missing
   role-permission facts, not only on non-zero exit.
5. Build the wire daemon and rerun the RxDB browser test suite without the
   previous wire-daemon skip evidence, or document the exact remaining blocker.
6. Rerun the broad Business OS/RxDB/JS gates after the smoke additions.

Phase evidence:

- Status: Complete
- Evidence links:
  - PASS 2026-06-17: `business-os-roles-permissions-ui` smoke mode added to
    `src/core/rxdb/tools/browser_rust_smoke.js`; syntax gate passed via
    `node --check src/core/rxdb/tools/browser_rust_smoke.js
    src/core/rxdb/tools/browser_rust_smoke_matrix.js
    src/apps/business-os/app.js src/apps/business-os/shared/permissions.js
    src/apps/business-os/shared/roles.js
    src/apps/business-os/shared/shell-permissions-ui.js`.
  - PASS 2026-06-17: Matrix requirements added to
    `src/core/rxdb/tools/browser_rust_smoke_matrix.js` for all
    role-permission evidence keys.
  - PASS 2026-06-17: `node src/apps/business-os/shared/roles.test.mjs &&
    node src/apps/business-os/shared/permissions.test.mjs &&
    node src/apps/business-os/shared/shell-permissions-ui.test.mjs &&
    node src/apps/business-os/scripts/assert-permissions-ui.mjs`.
  - PASS 2026-06-17: `cargo build --bin ctox --no-default-features
    --target-dir runtime/build/core-rxdb-integration-target`.
  - PASS 2026-06-17:
    `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js
    SMOKE_MODES=business-os-roles-permissions-ui SMOKE_MATRIX_ATTEMPTS=1
    SMOKE_PAGE_PATH=/index.html
    node src/core/rxdb/tools/browser_rust_smoke_matrix.js`.
    Evidence included target module `app-store`, scoped other module `ctox`,
    Teammitglied hide checks, exact source-view and modify grant checks,
    Owner context checks, Appbar source gate, exact-scope isolation,
    Owner/Admin role-option boundary, business-facing label check, reload
    verification, `auth_state=local-session`,
    `advanced_status=business-os-advanced-status-v1`, 0 browser errors,
    0 browser warnings, 0 request failures and 0 asset response errors.
  - PASS 2026-06-17:
    `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js
    SMOKE_MODES=business-os-ui-regression,business-os-roles-permissions-ui
    SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html
    node src/core/rxdb/tools/browser_rust_smoke_matrix.js`.
    The combined matrix passed; the existing `business-os-ui-regression` mode
    reported 0 browser errors, 0 request failures and 0 asset response errors,
    with one existing Chromium contenteditable/flex warning. The new
    role-permission mode reported 0 warnings.
  - PASS 2026-06-17:
    `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/cargo-target
    cargo build --release --example v15_wire_daemon` from
    `src/core/rxdb`.
  - PASS 2026-06-17: `node src/apps/business-os/rxdb/tests/run-all.mjs` -
    39 passed, 0 failed, 0 skipped after the wire daemon build.
  - PASS 2026-06-17: `cargo fmt --check --manifest-path
    src/core/rxdb/Cargo.toml`.
  - PASS 2026-06-17: `cargo test --manifest-path
    src/core/rxdb/Cargo.toml` - 239 unit tests, 30 conformance tests and
    doc-tests passed.
  - PASS 2026-06-17: `cargo test --bin ctox business_os` - 241 passed,
    0 failed.
- Notes:
  - This phase is additive. Existing Phase 0-6 behavior and guards remain
    intact.
  - The local smoke app has no login gate in the isolated RxDB smoke profile;
    the production gate therefore records the observed browser auth state as
    evidence and focuses on role/permission scoping inside the loaded
    Business OS session.
  - No files were patched in generated dist bundles, no HTTP fallback was added,
    and no environment-variable runtime toggle controls production behavior.

## Phase 8: Dynamic App Lifecycle Core

Goal: make the version-driven lifecycle rule visible and enforceable for
runtime-installed apps before the heavier publish workflow is built.

Status: Complete for the core slice.

Implemented scope:

1. Shared lifecycle projection for runtime-installed apps.
2. Shell, module appbar and App Store version/lifecycle badges.
3. Lifecycle drawer with current version, visibility state, collections and
   governance shortcuts.
4. Runtime-installed app visibility rule:
   - `0.x.y`: visible in normal discovery only to assigned
     App-Verantwortliche:r or exact `apps.view` grants.
   - `1.0.0+`: Team-visible by default.
   - explicit `restricted`: hidden from Team unless explicitly scoped.
   - invalid SemVer: private with warning.
5. Runtime-installed app `ctx.db` guard for `collection(name)`, collection
   properties and `raw`.
6. Smoke evidence for Shell visibility, badge/drawer rendering, reload and DB
   read/write denial.

Acceptance criteria:

- Teammitglied cannot discover private `0.x` runtime apps.
- App-Verantwortliche:r can discover private `0.x` runtime apps.
- Teammitglied can discover `1.0.0+` runtime apps unless restricted.
- Restricted released apps are not accidentally public.
- Runtime app code cannot bypass denied reads or writes through `ctx.db.raw`.

Required tests:

- `node src/apps/business-os/shared/app-lifecycle.test.mjs`
- `node src/apps/business-os/modules/app-store/app-store.test.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `SMOKE_MODES=business-os-dynamic-apps-ui ... browser_rust_smoke_matrix.js`
- `cargo test --bin ctox business_os`

## Phase 9: Native Lifecycle Projection And Migration

Goal: move lifecycle state from browser-only interpretation into durable native
catalog projection while preserving the existing WebRTC-only data plane.

Status: Complete.

Implemented scope:

1. Existing native catalog path now carries first-class lifecycle metadata:
   `lifecycle.visibility_state`, `lifecycle.audience`,
   `lifecycle.release_channel`, `lifecycle.current_semver`,
   `lifecycle.creator_user_id`, `lifecycle.responsible_user_ids`,
   `lifecycle.preview_grant_ids`, `lifecycle.preview_user_ids`,
   `lifecycle.release_required_checks`, `lifecycle.last_release_id` and
   `lifecycle.last_reviewed_at_ms`.
2. Storage decision accepted and implemented: no new RxDB collection. Native
   state is projected through the existing `business_module_catalog` document
   and mirrored in `governance.lifecycle`.
3. Backfill semantics are conservative:
   - packaged/core/starter modules => `packaged` / `system`.
   - missing or invalid installed-app SemVer => `private` with
     `warning_code=invalid_semver`.
   - `0.x.y` installed apps => `private`, or `preview` when explicit preview
     metadata/users/grants exist.
   - `1.0.0+` installed apps => `team`, unless metadata says `restricted`.
4. Creator and responsibility stay separate:
   - `business_module_versions.created_by` and latest release `created_by`
     remain audit/source metadata.
   - `business_module_acl` remains the App-Verantwortliche source.
   - exact `apps.modify`, `apps.source.view` and `apps.release` grants can add
     preview/build access without changing a user's global role.
5. Browser lifecycle code consumes projected native metadata first. In
   particular, projected `current_semver=null` is authoritative and prevents a
   stale manifest `version` from making an invalid installed app public.
6. Native RxDB module-catalog sync now handles recoverable projection write
   conflicts through the existing fallback repair/upsert path instead of
   logging a failed catalog sync.

Original source-validated scope:

1. Define durable lifecycle metadata on the existing module/catalog path:
   `visibility_state`, `audience`, `release_channel`, `current_semver`,
   `creator_user_id`, `responsible_user_ids`, `preview_grant_ids`,
   `release_required_checks`, `last_release_id`, `last_reviewed_at_ms`.
2. Decide whether metadata lives in existing native tables, a new native table
   projected into `business_module_catalog`, or the existing module manifest.
   If a new RxDB collection is proposed, record the schema/contract reason and
   regenerate contracts; default recommendation is projection through existing
   catalog/governance records.
3. Backfill installed apps:
   - missing or invalid version => private, warning state.
   - `0.x.y` => private unless explicit lifecycle metadata says preview.
   - `1.0.0+` => team unless metadata says restricted.
   - packaged/core modules => packaged/system lifecycle.
4. Persist creator/responsibility separately:
   - `created_by` remains audit metadata.
   - `business_module_acl` remains the stable App-Verantwortliche source.
5. Project lifecycle metadata through `module_governance_map` /
   `business_module_catalog` so Shell and App Store do not need divergent
   local inference.
6. Add migration/reconciliation logic for stale catalog rows and module
   manifests.

Acceptance criteria:

- A fresh browser and a reloaded browser see the same lifecycle state without
  synthetic JS-only state.
- Backfilled apps do not become more public than before migration.
- Invalid or missing SemVer cannot silently become Team-visible.
- Existing `business_module_releases`, `business_module_versions` and
  `business_module_acl` projections remain compatible.
- No HTTP data path, no dist patch and no process-env runtime toggle are added.

Required tests:

- PASS 2026-06-17: `rustfmt --edition 2021
  src/core/business_os/rxdb_peer.rs src/core/business_os/store.rs --check`.
- PASS 2026-06-17: `node --test
  src/apps/business-os/shared/app-lifecycle.test.mjs` - 9 passed.
- PASS 2026-06-17: `node --check
  src/apps/business-os/shared/app-lifecycle.js`.
- PASS 2026-06-17: `cargo test --bin ctox
  business_app_semver_major_matches_browser_plain_semver_contract`.
- PASS 2026-06-17: `cargo test --bin ctox module_catalog` - native catalog
  projection/backfill tests pass, including `0.x`, `1.0.0+`, missing SemVer,
  restricted metadata, App-Verantwortliche/projection grants and governance
  lifecycle projection.
- PASS 2026-06-17: `cargo test --bin ctox business_os` - 243 passed,
  0 failed.
- PASS 2026-06-17: `cargo build --bin ctox`.
- PASS 2026-06-17:
  `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright
  CTOX_BIN=runtime/build/cargo-target/debug/ctox
  SMOKE_MODE=business-os-dynamic-apps-ui SMOKE_PAGE_PATH=/index.html
  /Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node
  src/core/rxdb/tools/browser_rust_smoke.js`.
  Evidence includes private `0.x` hidden from Team, visible to builder,
  `1.0.0+` visible to Team, restricted app hidden from Team, lifecycle badges
  and drawer visible, invalid version private, DB read/write/raw denials,
  reload verified, `auth_state=local-session`, 0 browser warnings, 0 browser
  errors, 0 request failures and 0 asset response errors.

## Phase 10: App Store Publish And Data-Access Review Flow

Goal: make publishing a dynamic app to the Team a guided, auditable workflow
instead of a manual version/state convention.

Implementation scope:

1. Add an App Store release wizard for installed apps:
   - release target version.
   - SemVer validation.
   - source snapshot / module version selection.
   - release notes.
   - data-access review.
   - explicit reviewed data areas with read/write split.
   - responsible users.
   - rollback target.
   - final summary in business language: "Wer sieht die App?", "Welche Daten
     nutzt die App?", "Wie kann ich zurueck?"
2. Wire the wizard to the existing native command family:
   `ctox.module.release`, `ctox.module.rollback`,
   `ctox.module.rollback_version`, `ctox.source.list_snapshots` and
   `ctox.source.rollback_snapshot`.
   The wizard must dispatch through `business_commands` over RxDB/WebRTC.
   Settings release/rollback remains read-only diagnostics unless it is later
   upgraded to the same command payload and gates. Legacy HTTP module source/
   release/rollback endpoints must stay unreachable and must not be re-added
   to the control-plane allowlist.
3. Extend native release checks:
   - `apps.release` required for release.
   - `apps.rollback` required for rollback.
   - `ctox.module.list_versions` requires an authenticated actor plus
     module-scoped `apps.source.view`, `apps.release` or `apps.rollback`.
   - release cannot publish invalid SemVer.
   - Team release requires `>= 1.0.0`.
   - data-access review must be completed before Team release.
   - review collections must match the manifest collection contract.
   - reviewed read/write collections must be subsets of the manifest
     collections and must be reconciled with explicit `data.read`/`data.write`
     grants or an intentionally locked-state behavior.
   - review evidence must never create or imply a data grant.
   - failed release checks must persist a failed command outcome before any
     user-visible success state can render.
4. Store release review evidence in native state and project summary to the
   catalog/governance projection.
5. Ensure rollback returns visibility and data policy to a consistent state.
6. Remove or de-emphasize Settings-only release controls once App Store becomes
   the primary product surface, or keep Settings as expert fallback with the
   same gates.
7. Add App Store permission helpers for release and rollback. UI affordances
   must use projected `apps.release` and `apps.rollback`, not `apps.modify`.
8. Gate the existing Versions/Rollback dialog by rollback permission and show a
   disabled business-facing reason when rollback is not available.
9. Align the Settings release fallback with the same payload contract
   (`target_version`, data review, rollback target) or explicitly mark it as
   read-only/expert diagnostics until it is upgraded.
10. Define release/rollback consistency behavior:
    - module manifest update.
    - source snapshot selected for release.
    - rollback snapshot created before destructive restore.
    - release row status transitions.
    - projected catalog lifecycle after reload.
    - stale version rows and duplicate in-flight release/rollback commands.
    - failure after manifest/file write.
    - failure after SQLite release/source row write.
    - repair or compensation path before success UI renders.
    - Phase 10D1 implements stale source/rollback version ref rejection before
      manifest write, transactional release/source-version SQLite writes and
      manifest restore on injected release/rollback DB failures.
    - Phase 10D2 implements duplicate release command idempotency and a tested
      lifecycle projection repair command for canonical release records, RxDB
      release projection rows and the module catalog. Phase 15 remains
      responsible for broader operator recovery drills, support artifacts and
      runbooks.
11. Treat `ctox.module.release_version` as non-existent unless implemented.
    Current source-validated commands are `ctox.module.release`,
    `ctox.module.rollback`, `ctox.module.rollback_version`,
    `ctox.source.list_snapshots` and `ctox.source.rollback_snapshot`.
12. Promote App Store detail to the primary control plane for this flow with
    four first-class sections:
    - Version/Freigabe.
    - Sichtbarkeit.
    - Datenzugriff.
    - Verantwortliche/Aktivitaet.
    A raw `Collections:` line or README text is not sufficient production UX
    evidence.
13. Treat existing App Store `Versionen` as expert diagnostics until they share
    the Phase-10 payload. Settings release/rollback controls are read-only
    diagnostics and must not expose active publish/rollback dispatch until they
    are upgraded to the same payload and gates. Restore actions can stay
    visible only when each action is gated by projected `apps.rollback` with a
    disabled business reason for denied actors.

Acceptance criteria:

- App-Verantwortliche:r can publish an assigned app only when release checks
  pass.
- Teammitglied cannot publish or rollback.
- Admin/Owner can publish/rollback within policy.
- A user with exact `apps.release` but no `apps.modify` can release only the
  scoped app; a user with exact `apps.rollback` but no `apps.modify` can
  rollback only the scoped app.
- A user without source/release/rollback rights cannot list version timeline
  metadata through a crafted `business_commands` row.
- A `0.x` app cannot be published to Team without a `1.0.0+` target.
- Release review shows exactly which collections the app can read/write.
- Release review cannot list undeclared collections and cannot grant data
  access by itself.
- Failed release/rollback attempts render as failed command outcomes and never
  leave a success operation state in the App Store.
- Success UI renders only after the catalog projection confirms the release or
  rollback state is consistent.
- App Store and Settings do not call legacy HTTP source/release/rollback
  endpoints.
- Rollback is visible, auditable and tested.
- The user can understand the workflow without seeing raw permission names,
  grant JSON, manifest JSON or `scope_id`.

Required tests:

- Native release command tests for SemVer, permission, review-required and
  rollback behavior.
- Native rollback tests proving `apps.rollback` is sufficient and `apps.modify`
  is not required for rollback.
- Native `ctox.module.list_versions` tests proving source/release/rollback
  rights are sufficient and unauthenticated/unauthorized crafted commands are
  denied with `policy_decision`.
- Native failed-outcome tests for invalid SemVer, missing review,
  mismatched review collections, missing release version and missing source
  snapshot.
- Native data-review tests for undeclared read/write collections, grant
  reconciliation and locked-state behavior without widened access.
- Failure-injection and repair tests for manifest/file write success followed by
  DB failure, duplicate commands, stale version rows and stale projections. The
  stale-version and release/rollback DB-failure parts are covered by Phase 10D1;
  duplicate release replay and lifecycle projection repair are covered by Phase
  10D2; broader operator recovery drills remain Phase 15 work.
- Native projection tests for release review summary in
  `business_module_catalog.governance.lifecycle` and projected
  `business_module_releases`.
- App Store hook tests for publish wizard state transitions and disabled
  reasons.
- App Store hook tests for release payload builder, data-access review summary,
  can-release/can-rollback permission wrappers and rollback disabled reasons.
- Settings fallback tests proving it either sends the same release payload or
  does not expose a stale publish action.
- Command-bus tests or source guards proving App Store release actions use
  RxDB/WebRTC `business_commands`, Settings exposes no stale release action,
  and neither path reintroduces legacy HTTP endpoints.
- Browser/Rust `business-os-app-release-ui` smoke:
  create or seed private app, complete review, publish `1.0.0`, verify Team
  discovery after reload, rollback and verify state.
- Browser smoke must verify visible badges/drawer text, not only database rows:
  private `v0.x`, Team `v1.0.0`, reviewed data areas, rollback target,
  launcher/start-menu lifecycle/version badges and post-reload visibility.
- Audit tests for release, rollback, failed release denials and failed rollback
  outcomes.

Phase 10A/10B/10C/10D1/10E1/10E2 completed backend/static evidence plus
Phase 10E3 UI/static partial evidence:

- PASS 2026-06-17: `rustfmt --edition 2021 src/core/business_os/store.rs
  --check`.
- PASS 2026-06-17: `cargo test --bin ctox module_ -- --nocapture` - 31
  passed, 0 failed, covering:
  `module_release_rollback_command_uses_apps_rollback_without_modify_permission`,
  `module_source_rollback_version_uses_apps_rollback_without_modify_permission`,
  `module_rollback_commands_persist_failed_outcomes`,
  `module_list_versions_requires_source_release_or_rollback_rights`,
  `module_release_rejects_data_review_access_outside_manifest` and
  `module_release_stores_data_review_as_evidence_without_implied_grants`.
- PASS 2026-06-17: `cargo test --bin ctox module_release_ -- --nocapture` -
  5 passed, 0 failed, covering explicit Team data grants, missing grant
  rejection and locked-state release evidence.
- PASS 2026-06-17: `cargo test --bin ctox module_ -- --nocapture` - 33
  passed, 0 failed, after the data-review reconciliation update.
- PASS 2026-06-17: legacy HTTP module source/release/rollback handlers were
  removed from `src/core/business_os/server.rs`; the global
  `/api/business-os` data-route 410 gate remains in place.
- PASS 2026-06-17: `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
  proves no legacy route strings, direct source helper markers or direct
  release/rollback store calls exist in `server.rs`.
- PASS 2026-06-17: `node --check src/apps/business-os/scripts/assert-rxdb-only.mjs`.
- PASS 2026-06-17: `rustfmt --edition 2021
  src/core/business_os/server.rs --check`.
- PASS 2026-06-17: `cargo check --bin ctox`.
- PASS 2026-06-17: `cargo test --bin ctox module_ -- --nocapture` - 33
  passed, 0 failed, after removing the legacy HTTP handlers.
- PASS 2026-06-17: `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`.
- PASS 2026-06-17: `cargo test --manifest-path src/core/rxdb/Cargo.toml` -
  239 unit tests and 30 conformance tests passed.
- PASS WITH SKIPS 2026-06-17:
  `node src/apps/business-os/rxdb/tests/run-all.mjs` - 37 passed, 0 failed,
  2 skipped because the wire daemon was not built in this run. Do not use this
  as full Browser/Rust production smoke evidence.
- PASS 2026-06-17: Phase 10D1 `rustfmt --edition 2021
  src/core/business_os/store.rs --check`.
- PASS 2026-06-17: Phase 10D1
  `cargo test --bin ctox module_release_ -- --nocapture` - 8 passed,
  0 failed, covering `module_release_rejects_stale_source_and_rollback_version_refs_before_manifest_write`,
  `module_release_restores_manifest_when_release_db_write_fails` and
  `module_release_rollback_restores_manifest_when_status_update_fails`.
- PASS 2026-06-17: Phase 10D1
  `cargo test --bin ctox module_ -- --nocapture` - 36 passed, 0 failed.
- PASS 2026-06-17: Phase 10D1 `cargo check --bin ctox`.
- PASS 2026-06-17: Phase 10D1
  `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`.
- PASS 2026-06-17: Phase 10D1
  `cargo test --manifest-path src/core/rxdb/Cargo.toml` - 239 unit tests and
  30 conformance tests passed.
- PASS WITH SKIPS 2026-06-17: Phase 10D1
  `node src/apps/business-os/rxdb/tests/run-all.mjs` - 37 passed, 0 failed,
  2 skipped because the wire daemon was not built in this run. Do not use this
  as full Browser/Rust production smoke evidence.
- PASS 2026-06-17: Phase 10E1
  `cargo test --bin ctox module_release_ -- --nocapture` - 11 passed,
  0 failed, covering `module_release_and_rollback_write_business_event_audit`
  `module_release_failed_validation_writes_business_event_audit` and
  `module_release_rollback_failed_outcome_writes_business_event_audit`.
- PASS 2026-06-17: Phase 10E1
  `cargo test --bin ctox module_ -- --nocapture` - 39 passed, 0 failed.
- PASS 2026-06-17: Phase 10E1
  `rustfmt --edition 2021 src/core/business_os/store.rs --check`.
- PASS 2026-06-17: Phase 10E1
  `node src/apps/business-os/scripts/assert-rxdb-only.mjs` - RxDB-only
  contract OK.
- PASS 2026-06-17: Phase 10E1 `cargo check --bin ctox`.
- PASS 2026-06-17: Phase 10E1
  `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`.
- PASS 2026-06-17: Phase 10E1
  `cargo test --manifest-path src/core/rxdb/Cargo.toml` - 239 unit tests and
  30 conformance tests passed.
- PASS WITH SKIPS 2026-06-17: Phase 10E1
  `node src/apps/business-os/rxdb/tests/run-all.mjs` - 37 passed, 0 failed,
  2 skipped because the wire daemon was not built in this run. Do not use this
  as full Browser/Rust production smoke evidence.
- PASS 2026-06-17: Phase 10E2
  `cargo test --bin ctox module_catalog_projects_release_state_data_access_and_rollback_target -- --nocapture`
  - 1 passed, covering current release projection, rollback target projection,
  locked/granted data-area projection and post-rollback current release state.
- PASS 2026-06-17: Phase 10E2
  `cargo test --bin ctox module_release_ -- --nocapture` - 11 passed,
  preserving release/rollback command, review, consistency and audit coverage
  after projection changes.
- PASS 2026-06-17: Phase 10E2
  `cargo test --bin ctox module_ -- --nocapture` - 40 passed, 0 failed.
- PASS 2026-06-17: Phase 10E2
  `rustfmt --edition 2021 src/core/business_os/store.rs --check`.
- PASS 2026-06-17: Phase 10E2
  `node src/apps/business-os/scripts/assert-rxdb-only.mjs` - RxDB-only
  contract OK.
- PASS 2026-06-17: Phase 10E2 `cargo check --bin ctox`.
- PASS 2026-06-17: Phase 10E2
  `cargo test --manifest-path src/core/rxdb/Cargo.toml` - 239 unit tests and
  30 conformance tests passed.
- PASS WITH SKIPS 2026-06-17: Phase 10E2
  `node src/apps/business-os/rxdb/tests/run-all.mjs` - 37 passed, 0 failed,
  2 skipped (`cross-process-file-fetch-smoke` and `cross-process-wire-smoke`)
  because the wire daemon was not built in this run. Do not use this as full
  Browser/Rust production smoke evidence.
- PASS 2026-06-17: Phase 10E3 UI/static partial
  `node src/apps/business-os/shared/app-lifecycle.test.mjs` - 11 passed,
  covering shared release projection, rollback target labels and
  business-facing granted/locked data-area summaries.
- PASS 2026-06-17: Phase 10E3 UI/static partial
  `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 17
  passed, covering App Store release fact lines and release badges from native
  lifecycle projection.
- PASS 2026-06-17: Phase 10E3 UI/static partial
  `node --check src/apps/business-os/shared/app-lifecycle.js &&
  node --check src/apps/business-os/modules/app-store/index.js &&
  node --check src/apps/business-os/app.js`.
- PASS 2026-06-17: Phase 10E3 UI/static partial
  `node src/apps/business-os/scripts/assert-rxdb-only.mjs` - RxDB-only
  contract OK.
- PASS 2026-06-17: Phase 10E3 UI/static partial `git diff --check --`
  touched JS/CSS/docs paths and trailing-whitespace grep over the same files.
- PASS 2026-06-17: Phase 10G Settings fallback static downgrade
  `node src/apps/business-os/shared/react-settings.test.mjs` - 6 passed,
  covering disabled Settings Freigabe/Rollback diagnostics and a source guard
  against active stale Settings release/rollback dispatch controls.
- PASS 2026-06-17: Phase 10G Settings fallback static downgrade
  `node --check src/apps/business-os/shared/react-settings.js`.
- PASS 2026-06-17: Phase 10G Settings fallback Browser/Rust evidence
  `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`
  and
  `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-roles-permissions-ui SMOKE_MATRIX_ATTEMPTS=2 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=240000 BUSINESS_PORT=18988 SIGNALING_PORT=28988 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`
  - matrix OK with
  `business_os_roles_permissions_settings_release_fallback_readonly=1`,
  `startup_smoke_hook_reload_count=0`, `startup_smoke_hook_wait_ms=79`,
  `browser_error_count=0`, `browser_resource_404_count=0` and
  `browser_request_failure_count=0`.
- PASS 2026-06-17: Phase 10D2 backend consistency
  `rustfmt --edition 2021 src/core/business_os/store.rs
  src/core/business_os/rxdb_peer.rs --check`.
- PASS 2026-06-17: Phase 10D2
  `cargo test --bin ctox module_release_command_replay_does_not_duplicate_release_state --target-dir runtime/build/core-rxdb-integration-target -- --nocapture`
  - 1 passed, proving replayed `ctox.module.release` does not add a duplicate
  release row, manual source-version row or release audit event.
- PASS 2026-06-17: Phase 10D2
  `cargo test --bin ctox module_lifecycle_projection_repair_resyncs_releases_and_catalog --target-dir runtime/build/core-rxdb-integration-target -- --nocapture`
  - 1 passed, proving `ctox.module.repair_lifecycle_projection` restores the
  canonical release projection, RxDB release projection and module catalog
  lifecycle state from the real runtime-installed app root.
- PASS 2026-06-17: Phase 10D2
  `cargo test --bin ctox module_release_ --target-dir runtime/build/core-rxdb-integration-target -- --nocapture`
  - 12 passed, 0 failed.
- PASS 2026-06-17: Phase 10D2
  `cargo test --bin ctox module_ --target-dir runtime/build/core-rxdb-integration-target -- --nocapture`
  - 42 passed, 0 failed.
- PASS 2026-06-17: Phase 10D2
  `node src/apps/business-os/scripts/assert-rxdb-only.mjs` - RxDB-only contract
  OK.
- PASS 2026-06-17: Phase 10D2
  `cargo check --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`.

Historical Phase 10 open list before the later Phase 10 closeout:

- Settings, Shell and App Store consume projected release review/status/rollback
  target/data-area summary in static/UI helper tests, but reload Browser/Rust
  evidence is still required and success must not render from command result
  alone.
- Settings Activity/browser evidence for release, rollback and failed
  validation audit labels; backend `business_events` are covered by Phase
  10E1.
- App Store publish wizard and rollback UX.
- Browser/Rust publish/reload/rollback smoke and audit evidence.

Current status: these Phase 10 items are closed by `P10-UI-WIZARD`,
`P10-BE-RELEASE-PROJECTION`, `P10-SETTINGS-FALLBACK` and `P10-BE-AUDIT`.
Historical checkpoint: after Phase 10, Phases 11-16 were still the
full-product production-readiness work. The current state is tracked in the
Phase Tracker above.

## Phase 11: Audience, Preview And Responsibility Management

Goal: give non-technical controls for who can see private, preview, team and
restricted apps without exposing raw permission names.

Implementation scope:

1. Add App Store lifecycle panel controls:
   - `Nur App-Verantwortliche`
   - `Vorschaugruppe`
   - `Team`
   - `Eingeschraenkt`
2. Model preview and restricted audiences through existing exact grants and
   `business_module_acl` where possible. Add only the minimum new native state
   needed for durable named audiences.
   Previous governance-derived visibility from `apps.modify`,
   `apps.source.view`, `apps.release` or `apps.rollback` has been removed from
   the production audience model; durable preview/restricted visibility must
   use `apps.view` or dedicated audience state that does not imply edit/source,
   release or data access.
3. Define durable visibility semantics before implementation:
   - "Nur App-Verantwortliche" uses creator/responsibility state.
   - "Vorschaugruppe" uses a visibility grant or native audience state that
     does not imply `apps.modify`, `apps.source.view`, `apps.release` or data
     access.
   - "Team" follows released `>= 1.0.0` lifecycle state.
   - "Eingeschraenkt" uses explicit user/role audience state.
4. Support user and role audience selection with business-facing labels.
5. Add "Zugriff anfragen" only if the command/approval path is implemented;
   otherwise show a read-only reason.
6. Keep visibility grants distinct from data grants. Seeing an app does not
   imply collection read/write.
7. Add responsibility management:
   - assign/remove App-Verantwortliche.
   - audit every change.
   - prevent orphaned private app when the last responsible user is removed,
     unless Owner/Admin explicitly accepts recovery responsibility.
8. Replace the current browser-only preview inference where `preview_user_ids`
   changes lifecycle display but does not by itself grant visibility. The
   shipped model must make display state and access state match.
9. Make lifecycle badge and drawer interactions permission-aware:
   - actors without visibility-management rights get a tooltip/read-only state.
   - managers get clear management affordances and shortcuts; publish, preview,
     restrict and responsibility changes stay on the App Store control plane.
   - launcher/start-menu app-choice items render the same version/lifecycle
     signal as Shell tabs, Appbar and App Store cards.
   - data sections show business data-area labels plus read/write status, not
     only raw collection ids.
   - truncated chips require `aria-label`/`title` and the drawer/detail must
     expose the full label.
10. Add deep-link handling for non-public apps:
   - out-of-audience users see a locked state or are redirected with a clear
     reason.
   - in-audience users can open the app but still see locked data areas without
     data grants.
11. Keep App Store detail aligned with the same audience model. The
    Sichtbarkeit section is the durable management surface; badge click is the
    shortcut, not the only control.

Acceptance criteria:

- Preview users can see/open the app but only within their data grants.
- Restricted released apps stay hidden or locked for out-of-audience users
  after reload.
- Audience changes are auditable and reversible.
- UI never shows raw `founder`, `data.read`, `scope_id` or grant JSON.
- Removing the original creator does not orphan the app if another
  App-Verantwortliche:r exists.
- Audience state survives catalog reload, browser reload and fresh profile.
- A preview/restricted visibility grant never grants source editing, release,
  rollback or collection data access by accident.
- Lifecycle badge behavior matches permissions: denied actors see read-only
  state/reason; managers see available actions; labels remain business-facing
  and accessible.

Required tests:

- Rust tests for audience/responsibility command persistence and audit.
- Rust tests for explicit visibility grant/audience semantics separate from
  `apps.modify` and `data.read`/`data.write`.
- JS tests for audience UI labels and disabled reasons.
- JS tests for lifecycle badge/drawer read-only vs manager behavior,
  launcher/start-menu badge rendering and full business data-area labels.
- JS lifecycle tests proving preview audience state and actual visibility
  authorization stay aligned.
- Browser/Rust `business-os-app-audience-ui` smoke across Owner/Admin/
  App-Verantwortliche:r/Teammitglied.
- Reload and fresh-profile browser checks for preview/restricted app
  visibility.
- Deep-link browser checks for out-of-audience users.

## Phase 12: Agent And App-Scope UX Parity

Goal: make AI/agent interaction follow the same app lifecycle, data and
external-effect rules as human users, with visible scope before action.

Implementation scope:

1. Surface active actor in AI-assisted app controls:
   "Handelt als <user/agent>" and active app/data scopes.
2. Pass app lifecycle and data-scope context into app-mode prompts and MCP
   delegated actions.
   MCP `list_modules`, `get_module`, `open_link`, `propose_action` and
   `execute_action` must evaluate app lifecycle/audience visibility separately
   from `data.read`/`data.write`.
3. Ensure service actors remain normal Business OS actors:
   persisted `business_users` role or exact grants only.
4. Add UI for exact app/data grants for agent actors where product requires it.
5. Show blocked reasons for agents in the same business language as human
   users.
6. Keep external effects behind `external.approve`; do not broaden MCP v1
   external-effect behavior without a separate risk model and tests.
7. Add a per-app AI capability panel in App Store detail or app settings:
   - which agents can see the app.
   - which agents can read/write which data areas.
   - whether the agent can propose source changes.
   - whether external approvals are required.
8. Ensure right-click/App-Store AI actions reuse the selected app context and do
   not fall back to the App Store module id.
9. Record blocked agent attempts with the same policy-decision shape used for
   human actors, plus the resolved service actor id.
10. Cover every AI-assisted entry point with the same visible scope panel:
    - global right-click.
    - App Store context chat.
    - Business Chat.
    - Coding Agents handoff.
    - MCP delegation.
    The panel must show `Handelt als`, selected app, lifecycle state,
    read/write data areas and external-effect status before submit.
11. Submitted `client_context` must match the visible selected app/scope. Tests
    must fail if the UI displays one app but submits the App Store or global
    module id.

Phase 12A status on 2026-06-17: the MCP backend split is implemented.
`list_modules` now uses lifecycle/App-Visibility (`public` or exact
`apps.view`) before descriptor exposure; module detail, entity, action and
proposal tools require app visibility before `data.read`; module links require
app visibility without implying data access; action execution requires app
visibility before `data.write`.

Phase 12B/12C first browser status on 2026-06-17: the global right-click CTOX
menu now shows a business-facing scope panel before submit and sends the same
`agentScope` object as `client_context.visible_scope`. The command bus now
canonicalizes module/app/action/mode/target/record/scope aliases at the RxDB
boundary without treating browser scope as authorization, and Coding Agents add
provider/workspace/session external-scope context. Business Chat scheduled
commands now preserve an existing `chat.contextMeta.client_context` before
adding fresh message and attachment metadata, and Business Chat renders the
preserved `visible_scope` rows in the chat window when a chat was opened from a
scoped context. App Store context chat now renders/builds the same selected-app
scope and submits it as `client_context.visible_scope`, including data-mode
prompts, while disallowed app-modify requests are downgraded to data mode.
Browser/Rust `business-os-agent-scope-ui` now proves the global right-click,
App Store context-chat and Business Chat rendered-scope visible-vs-submitted
comparison, hidden app denial, data-read grant boundary, write denial,
persisted command audit and Settings read-only Owner/Admin grant boundary.
Native/MCP audit metadata parity is also implemented: native policy events
persist redacted scope-only `client_context`, MCP events persist
`business_scope`, and tests prove prompt, selected text and MCP payload content
are not copied into audit metadata. Phase 12 is locally complete; Phases 13-16
remain open for full-product production readiness.

Acceptance criteria:

- Agent cannot see or act on private `0.x` apps without app responsibility or
  exact grants.
- Agent cannot read/write app data without `data.read`/`data.write`.
- User can see which app and collection scopes an agent will use before
  irreversible action.
- MCP and browser app-mode denials produce compatible audit evidence.
- Agent source-change proposals require app/source permission and do not imply
  release permission.
- Agent data reads/writes produce visible app/data scope in the prompt or
  confirmation surface before execution.
- A data grant alone does not make an app visible to MCP/agent flows; app
  lifecycle/audience visibility must pass first.
- Visible actor/app/data/external-effect scope matches submitted
  `client_context`.

Required tests:

- MCP policy tests for dynamic app visibility and data grants.
- MCP tests for private `0.x`, preview and restricted dynamic apps.
- MCP tests proving app visibility and data grants are independent decisions.
- Browser tests for app-mode controls showing actor and scope.
- Browser/Rust smoke that delegates an agent/app action with no grant, exact
  read grant and exact write grant.
- Audit tests for delegated allowed/denied app actions.
- App Store/right-click tests proving selected-app context is preserved for AI
  actions.
- Browser tests for global right-click, App Store context chat, Business Chat,
  Coding Agents handoff and MCP delegation scope panels.

## Phase 13: Packaged/Core Module Data-Isolation Migration

Goal: remove the remaining compatibility gap where packaged/core modules still
receive broad DB access, without breaking existing core workflows.

Implementation scope:

1. Inventory every packaged/core module and its collections from
   `module.json`/schema files.
   The inventory must be committed as an artifact, for example
   `docs/business-os-packaged-module-isolation-inventory.md`.
2. Classify each module:
   - can use guarded facade immediately.
   - needs explicit internal system exception.
   - needs code changes to request collection handles explicitly.
3. Migrate modules incrementally to the same guarded facade used by
   runtime-installed apps.
4. Add static linting that prevents new direct `ctx.db.raw` or unscoped
   collection access unless the module is explicitly allowlisted.
5. Keep app-level locked-state rendering consistent for denied data.
6. Document any permanent system exceptions with owner, reason and tests.
7. Re-verify runtime-installed app isolation in the real Shell path:
   `openModule(mod)` -> `createModuleContext(mod)` -> guarded DB facade. The
   active module must be passed into the facade; a global unscoped facade is a
   blocker.
8. Cover collection-property handles, cached handles and `ctx.db.raw`, not only
   direct `collection(name)` calls.
9. Add a narrow exception mechanism only for explicit system modules. Each
   exception needs:
   - module id.
   - owner.
   - collection/surface.
   - reason.
   - expiry or review cadence.
   - automated test proving the exception is narrow.
10. Inventory and scope Desktop-, Chat-, Report- and internal shell facades.
    Phase 13H closed the naked-facade part by replacing them with
    `createScopedSystemDbFacade(scopeName, collectionNames)` allowlists instead
    of a generic active-module scope:
    - Settings drawer: narrow system/admin facade for
      `business_module_catalog` and `business_commands`, with role/permission
      checks.
    - Desktop app window context: split by `appId`, because Browser,
      Code-Editor, Creator, Explorer and File-Viewer need different
      collection allowlists.
    - Business Chat companion: narrow system facade for `business_chats`,
      `business_commands`, `ctox_queue_tasks`, `desktop_files` and
      `desktop_file_chunks`, with actor/owner row filtering and attachment-only
      file access.
    - Business Reporter companion: narrow reporter system facade for report,
      bug-report and command projections, with active-module metadata but not a
      broad active-module DB facade.
    - Inventory source-path line numbers should stay fresh enough for review;
      the current guard validates facade identities but strips line numbers.
11. Browser/Rust smoke must mount a real runtime-installed app through the
    normal Shell path, reload it, then attempt bypasses through:
    - `ctx.db.collection(name)`.
    - `ctx.db.<collection>`.
    - cached collection handles captured before permission changes.
    - `ctx.db.raw`.
    Smoke-hook-only verification is not sufficient for phase completion.
12. Define the runtime safety boundary for installed/generated app JS:
    - whether runtime apps are trusted same-origin code or sandboxed code.
    - allowed `fetch`, dynamic `import`, worker, storage and navigation
      behavior.
    - which capabilities require explicit app permissions, user confirmation
      or external approval.
    - which behaviors are forbidden by static/conformance guards.
13. Add negative generated-app fixtures that try to bypass product boundaries
    through direct network calls, browser storage, global Shell state,
    unscoped DB references, external-effect commands and stale cached handles.
14. Scope browser storage keys where user/workspace relevant and document
    which keys are safe UI preferences. Permission, lifecycle, release,
    audience, tenant and data-grant decisions must never depend on browser
    storage values.

Acceptance criteria:

- No user-facing packaged module bypasses `data.read`/`data.write` by default.
- System exceptions are narrow, documented and covered by tests.
- Module conformance fails on new unguarded raw DB access.
- Existing core workflows still pass module-specific tests and UI regression
  smoke.
- A runtime-installed app loaded through the real Shell cannot access
  undeclared or ungranted collections through `ctx.db`, collection properties,
  cached handles or `ctx.db.raw`.
- No unscoped Desktop/Chat/Report/internal facade remains; any future broad
  facade requires a committed system-exception row and automated test.
- Installed/generated app code has an explicit trusted-code or sandboxed-code
  product contract, and forbidden capabilities fail in static and browser
  negative fixtures.
- Browser storage reset, copy or manipulation cannot widen app visibility,
  roles, release state, audience, tenant scope or data grants.

Required tests:

- `assert-module-conformance.mjs` extended for raw DB access rules.
- Shell/module-context tests proving the active module reaches
  `createLiveDbFacade`.
- Module tests for migrated apps.
- Negative tests for collection property and raw-handle bypass attempts.
- Negative tests for cached handles and permission changes after reload.
- Inventory tests for Desktop/Chat/Report/internal facade allowlists and
  unscoped-facade regressions.
- Browser/Rust UI regression matrix after each migration batch.
- Broad `cargo test --bin ctox business_os` and RxDB run-all gates.
- Inventory artifact review test or script that fails when a packaged/core
  module is missing from the classification.
- Runtime capability inventory test and negative generated-app fixtures for
  fetch/import/storage/external-effect bypass attempts.
- JS storage-boundary tests for scoped preference keys and non-authoritative
  permission/lifecycle state.
- Browser/Rust runtime-safety and storage-boundary smoke after reload and fresh
  profile.

## Phase 14: Production Browser E2E Auth, Tenant And Reload Matrix

Goal: prove the feature as a browser product, not only as policy/backend code.

Implementation scope:

1. Add Browser/Rust smoke modes for:
   - login.
   - authenticated reload.
   - logout.
   - logged-out reload.
   - blocked protected shell access after logout.
   - fresh profile with no IndexedDB/localStorage/sessionStorage.
   - release workflow.
   - audience/deep-link workflow.
   - agent-scope workflow.
2. Add tenant/workspace scoping scenarios:
   - user sees only allowed modules.
   - user sees only allowed app data.
   - user cannot deep-link to restricted/private app.
   - source/release/audience controls are hidden or disabled correctly.
3. Verify visible data, not only database rows:
   app names, version badges, counts, details, filters, empty states and
   locked states.
4. Make console errors, failed requests, 404s, WebRTC/RxDB startup failures,
   stuck loading states and unbudgeted warnings hard failures.
5. Capture exact tested URL, auth state, browser context, role, tenant,
   evidence keys and diagnostics in the smoke matrix.
6. Register new modes in `browser_rust_smoke.js` and
   `browser_rust_smoke_matrix.js`; matrix documentation alone is not enough.
7. Configure authenticated smoke users through typed runtime config or persisted
   test-root state. Do not add new process-env production runtime toggles.
8. Define the tenant/workspace model under test. If the product still uses a
   single local workspace, the smoke must explicitly state that and prove no
   cross-module/scope leakage inside that workspace; hosted multi-tenant claims
   need separate hosted evidence.
9. Run each story in a clean browser context and at least one fresh profile with
   empty storage.
10. Run desktop and narrow viewport checks for lifecycle/version/privacy UI:
    - `Privat`.
    - `Vorschau`.
    - `Team`.
    - `Eingeschraenkt`.
    - `App-Verantwortliche:r`.
    - disabled release/rollback/audience reasons.
    Chips may truncate only when `aria-label`/`title` and drawer/detail
    surfaces expose the full business wording without overlap.
11. Browser evidence must include screenshots or DOM/text assertions for the
    visible app names, version badges, lifecycle badges, disabled reasons,
    drawer/detail sections and no-overlap checks in both viewport classes.
12. Add storage-boundary assertions:
    - cleared storage starts from authoritative projected state.
    - copied storage from another actor/workspace cannot widen access.
    - local/session storage mutations cannot create release success, audience
      membership or data grants.
13. Add representative scale fixtures for:
    - app catalog size.
    - preview/restricted audience size.
    - grant count.
    - release history length.
    - audit event volume.
    Startup, sync, release/rollback, audience update and diagnostics actions
    need explicit time budgets and visible loading/progress/error states.

Acceptance criteria:

- All representative roles pass in clean browser profiles.
- Reload preserves correct lifecycle and permissions.
- Logout removes protected access.
- A fresh browser can reach a correct state without manual cache/local-storage
  setup.
- No cross-tenant app, data, source or governance leakage is visible.
- The smoke result fails when expected evidence keys are missing, even if the
  browser process exits successfully.
- Browser console warnings are either zero or explicitly listed in the warning
  budget with owner, reason and expiry.
- Long German business labels remain readable or accessibly expanded in
  launcher/start-menu, Shell, App Store and Settings across desktop and narrow
  viewports.
- Browser storage cannot alter effective permissions, lifecycle/audience state
  or tenant/workspace scope.
- Scale fixtures stay within configured startup/sync/action budgets or render
  clear progress/error states and fail the release gate.

Required tests:

- Existing/passing `business-os-app-release-ui` Browser/Rust smoke.
- Existing/passing `business-os-app-audience-ui` Browser/Rust smoke.
- Existing/passing `business-os-agent-scope-ui` Browser/Rust smoke for the
  global right-click agent-scope story.
- New `business-os-auth-scope-ui` Browser/Rust smoke.
- New `business-os-fresh-profile-ui` Browser/Rust smoke.
- New storage-boundary assertions inside auth/fresh-profile smoke.
- New scale/performance fixture or mode with mode-duration, startup and sync
  budgets enforced by the smoke matrix.
- Existing `business-os-ui-regression`, `business-os-roles-permissions-ui`
  and `business-os-dynamic-apps-ui` remain green.
- Matrix evidence requirements for every auth/scope fact.
- Desktop and narrow viewport visual/text assertions for lifecycle chips,
  launcher/start-menu badges, disabled reasons and drawer/detail full labels.
- Negative deep-link and logged-out protected-shell checks.

## Phase 15: Observability, Audit Retention And Support Recovery

Goal: make failures diagnosable and recoverable by operators without reading
raw database tables or guessing at grants.

Implementation scope:

1. Add operator diagnostics for:
   - app lifecycle state.
   - release checks.
   - data grants.
   - audience grants.
   - actor effective permissions.
   - WebRTC/RxDB health.
   - source snapshot/release snapshot alignment.
   - rollback readiness.
   - module DB-isolation classification.
2. Add audit/export coverage for:
   - release/rollback.
   - failed release/rollback validation.
   - lifecycle visibility changes.
   - preview/restricted audience changes.
   - data-access review changes.
   - agent delegated app actions.
   - policy denials and allowed high-risk decisions.
3. Define retention and volume controls for `business_events` and MCP audit
   events.
   Native `business_events` now have a support-safe export-before-prune command
   for expired rows and typed persisted retention days; remaining work is
   volume/operations policy where required plus recovery/backup drills. MCP
   audit retention is now stored in typed native policy state
   `business_os.mcp_policy.v1`; legacy `CTOX_BUSINESS_OS_MCP_*` runtime-env
   values are read only as migration fallback until a typed policy exists.
4. Add recovery commands/runbook steps:
   - private app orphan recovery.
   - broken release rollback.
   - bad data grant removal.
   - stale catalog repair.
   - native peer/RxDB projection repair.
   - manifest/release-row divergence repair.
   - source snapshot restore when release rollback is insufficient.
5. Add support-safe redaction rules so diagnostics do not leak message body,
   secrets, tokens or sensitive record fields.
6. Add an operator-facing "Warum?" explanation path for a selected actor/app:
   - can see app.
   - can open app.
   - can edit source.
   - can release/rollback.
   - can read/write each declared data area.
7. Support export-before-prune for audit retention. Pruning without a verified
   support-safe export is not allowed.
8. Define a diagnostics artifact format for support cases:
   - actor/app/module ids.
   - effective permissions.
   - lifecycle/audience state.
   - release/review/rollback consistency status.
   - redaction manifest.
   - generated-at timestamp and product version.
   It must not include prompt bodies, message bodies, raw record payloads,
   secrets or tokens by default.
9. Add backup/restore drill coverage for:
   - native SQLite runtime store.
   - module manifests and installed app roots.
   - source snapshots and release rows.
   - audit/export files and retention settings.
   - rollback target restoration after restore.
   Current native slice: `ctox business-os backup restore-drill [--module <id>]`
   writes raw sensitive snapshots under `runtime/backup`, restores them into an
   isolated root and validates support-safe restored-state facts. It is not yet
   an active production-root restore workflow.
10. Define the support artifact schema separately from ad hoc logs. Support
    exports must be machine-validated and safe to attach to tickets by default.

Acceptance criteria:

- Operator can explain why an actor can or cannot see/use/change an app.
- Audit export contains enough evidence for release and access decisions.
- Retention does not grow unbounded without operator control.
- Recovery drills are tested and documented.
- Diagnostics preserve WebRTC-only data boundary and redact sensitive content.
- Diagnostics never expose prompt bodies, record payload bodies, secrets,
  tokens or sensitive field values unless explicitly whitelisted for support.
- Retention controls are configurable through typed runtime config or native
  store state, not process-env production toggles.
- MCP audit retention uses typed native policy state, with runtime-env policy
  documented only as a legacy migration fallback.
- Support diagnostics can be exported and attached to a support ticket without
  exposing sensitive content by default.
- Backup/restore or documented manual recovery restores app visibility, data
  grants, release state, rollback target and audit evidence conservatively. The
  current CLI drill covers isolated native restore validation; active-root
  restore and browser unsynced-state recovery remain release blockers.
- Support artifact schema validation fails on prompt bodies, message bodies,
  raw record payloads, secrets, tokens or unredacted sensitive fields.

Required tests:

- Rust tests for diagnostics command output and redaction.
- Audit export tests for new event families.
- Recovery drill tests for stale catalog, orphan private app and bad grant.
- Recovery drill tests for manifest/release-row divergence and bad rollback
  target.
- Retention prune tests proving export-before-prune and redaction.
- Tests covering MCP typed retention state and legacy migration fallback.
- Diagnostics artifact schema/redaction tests.
- Backup/restore drill tests or documented manual drill evidence for native
  store, manifests, snapshots, release rows and audit exports. The native
  isolated drill test is present; active-root/browser/WebRTC restore proof is
  still required before closing Phase 15.
- Browser smoke for Activity/diagnostics rendering with business-facing labels.

## Phase 16: Release Packaging, CI Gates And Rollout Runbook

Goal: make the full roles/permissions and dynamic-app lifecycle work shippable
through the normal CTOX release process.

Implementation scope:

1. Promote required tests into CI/release gates:
   - Rust `business_os` tests.
   - `src/core/rxdb` unit/conformance tests.
   - Business OS RxDB JS run-all with zero relevant skips.
   - JS permission/lifecycle/module tests.
   - Browser/Rust smoke matrix for UI regression, roles/permissions,
     dynamic apps, auth/scope, fresh profile and release workflow.
   - module conformance/inventory checks.
   - App Store release/audience/agent hook tests.
   The tag/release workflow must depend on this Business OS release-gate job
   before any release artifact is uploaded.
2. Add release checklist to `docs/business-os-roles-permissions-rollout.md`.
3. Add customer/operator documentation:
   - role meanings.
   - app lifecycle labels.
   - publishing apps.
   - preview/restricted sharing.
   - agent scopes.
   - recovery.
4. Add migration notes for existing installs and expected first-run/backfill
   behavior.
5. Add downgrade/rollback notes for module lifecycle metadata and release
   records.
6. Define ship blockers and explicit allowed warning budgets.
7. Add a release artifact or machine-readable summary for the smoke matrix. It
   must include tested URL, git revision, role, auth mode, browser profile mode,
   smoke mode, evidence keys, warning budget and result.
   The artifact must be written to a fixed path, uploaded by CI/release and
   schema-validated. A process exit code without the expected artifact is a
   release failure.
8. Add migration/backfill checks for existing installs:
   - no `0.x` app becomes Team-visible.
   - missing versions stay private with a warning.
   - existing release rows project lifecycle without widening data grants.
   - failed migration can be retried safely.
   These checks require committed legacy fixtures/stores for private `0.x`,
   missing version, invalid SemVer, released `1.x`, restricted and preview
   apps.
9. Add rollback/downgrade notes for Phase 10-15 state:
   - release review snapshot.
   - audience state/grants.
   - agent grants.
   - packaged-module exception inventory.
   - audit retention settings.
10. Define the exact hosted-production boundary. Local configured-auth evidence
    is not the same as managed/public hosted production evidence.
11. Document and CI-enforce the clean-checkout Node dependency bootstrap for
    Business OS JS tests. `esbuild`, Playwright and module-test dependencies
    must come from declared install steps, not host-global packages or local
    symlinks.
12. Add a CI guard that fails when required Browser/Rust smoke modes are absent
    from `browser_rust_smoke.js` or `browser_rust_smoke_matrix.js`.
13. Add a blocking security/privacy signoff checklist covering:
    - dynamic app runtime code and runtime capability boundary.
    - app source visibility and source snapshot access.
    - data-access review and locked-state UX.
    - MCP/agent scopes and submitted `client_context`.
    - audit/support export redaction.
    - external-effect approval boundaries.
    - release artifact integrity and smoke artifact authenticity.
14. Add customer/operator documentation with screenshots or text references
    matching the implemented UI for:
    - Owner/Admin/App-Verantwortliche:r/Teammitglied.
    - `Privat`, `Vorschau`, `Team`, `Eingeschraenkt`.
    - publishing and rollback.
    - data review and locked data states.
    - preview/restricted sharing.
    - agent scopes and external approvals.
    - support diagnostics, backup/restore and recovery.

Acceptance criteria:

- A clean checkout can run the documented release gate commands.
- CI fails on missing Browser/Rust smoke evidence, not only on process exit.
- CI fails when the fixed-path smoke artifact is absent, malformed or missing
  a required mode/evidence key/warning budget field.
- Successful final production smoke attempts include non-empty auth state,
  actor role, browser profile context, tenant scope and the expected advanced
  status, and the artifact validator rejects budget breaches.
- Release docs state exactly what is production-ready and what is not.
- Existing installs migrate without making private apps public.
- Rollback path is documented and tested.
- Release package cannot ship with undocumented smoke skips, stale Settings
  publish controls, unclassified module isolation exceptions or unbudgeted
  browser console/network failures.
- A clean checkout can install JS test dependencies and run Business OS JS
  gates without undeclared host state.
- Security/privacy signoff blocks release artifacts until dynamic-app runtime,
  app source, data review, agent scope, audit export, external-effect and
  artifact-integrity checks are recorded.
- Customer/operator docs match current UI labels and do not describe future
  controls as available.

Required tests:

- CI-equivalent local command bundle documented with exact commands.
- `git diff --check` and whitespace hygiene for touched docs/code.
- Release-smoke result artifact or summary file retained by the matrix.
- Smoke artifact schema validation test.
- Manual operator checklist dry-run against a local instance.
- Migration dry-run against seeded legacy installs with private `0.x`, missing
  version, released `1.x`, restricted and preview apps.
- CI test that fails if required smoke modes are absent from the matrix.
- CI/local test for declared JS dependency bootstrap on a clean checkout.
- Release workflow dependency test or dry-run proving artifacts cannot upload
  before Business OS production gates pass.
- Security/privacy checklist validation or release-script guard proving the
  signoff artifact exists before upload.
- Customer/operator docs dry-run against a local instance, including label
  checks for role names, lifecycle badges, locked data states and recovery
  actions.

## Production Extension Test Matrix

Each new phase must update this matrix when implemented.

| Area | Phase 8 | Phase 9 | Phase 10 | Phase 11 | Phase 12 | Phase 13 | Phase 14 | Phase 15 | Phase 16 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Lifecycle projection | PASS browser core helper and smoke | PASS native catalog projection/backfill plus browser projected-metadata smoke | PASS failed release/rollback outcomes, evidence-only review snapshot, manifest/SQLite stale-version consistency, duplicate release replay, lifecycle projection repair, native `release_state`/`rollback_target`/`data_access` projection and Browser/Rust reload consumption | PASS 11B legacy preview hints backfill to durable `apps.view`, projected preview users derive from active grants, and audience smoke verifies reload/fresh-profile visibility | MUST project active actor and app/data scopes for human and agent actions | MUST project packaged-module isolation classification and exceptions | MUST survive reload and fresh profile with correct lifecycle state | MUST explain projection source and stale/repair state | MUST be a release gate |
| Release workflow | N/A core slice | PASS release/version summary projected when native release rows exist; full wizard still Phase 10 | PASS backend list-version gate, rollback permission gate, failed outcomes, no-HTTP dispatch guard, 10D1 manifest/SQLite failure handling, 10D2 duplicate/repair consistency, 10E1 native release/rollback audit events, 10E2 native release-state projection, 10G Settings read-only fallback, App Store publish/rollback wizard and Browser/Rust release smoke | MUST preserve audience across release and rollback | MUST keep agent visibility/release permissions separate | MUST keep core-module compatibility explicit | MUST pass browser publish/rollback story | MUST include release/rollback diagnostics and recovery drills | MUST include release checklist, release workflow gate and smoke artifact |
| Data access | PASS guarded runtime-installed helper/smoke-hook path; normal dynamic `createModuleContext(mod)` and persisted `openModule`/reload facade paths have later Phase 13B evidence | PASS lifecycle projection preserves data-review placeholder and does not widen data grants | PASS manifest-subset validation, no implied grants, Team data grant/locked-state reconciliation, native granted/locked data-area projection and App Store browser release data-review rendering | PASS 11A/11B/11D proves `apps.view` does not imply edit/source/data, audience storage cannot widen visibility, hidden direct opens are blocked before module import, and lifecycle drawer/launcher labels stay business-facing for manager/read-only actors | MUST prove agent read/write scopes before action and after app visibility passes | PARTIAL PASS 13A inventory guard, 13B real Shell guarded facade, and 13D static drift guard; MUST still migrate or explicitly except packaged/core modules and define runtime/storage boundaries | MUST verify visible data scope E2E | MUST export diagnostics with redaction | MUST be a CI gate |
| Audience/preview | PASS restricted hidden from Team | PASS projected private/preview/team/restricted audience fields from manifest/grants/ACL | PASS Team release audience summary for App Store publish; durable preview/restricted visibility semantics remain Phase 11 | PASS 11A/11B/11C/11D `apps.view` separates non-public visibility from edit/source/release/data, legacy preview hints migrate to durable grants, private/preview/restricted out-of-audience states are hidden, direct hidden opens are locked, reload/fresh-profile/storage-boundary audience smoke passes, private runtime app responsibility cannot become orphaned without explicit recovery acceptance, and launcher/drawer UI shows manager vs read-only states | MUST apply the same audience to agents and MCP visibility | N/A except migrated modules must respect audience | MUST pass browser role/tenant/deep-link matrix | MUST include audience audit/recovery | MUST document customer-facing labels |
| Auth/tenant/browser | PASS local-session smoke only | PASS local-session reload smoke for projected lifecycle; configured auth remains Phase 14 | PASS local-session Browser/Rust release story with clean browser context and tenant-scope evidence; configured auth/tenant remains Phase 14 | PASS `business-os-app-audience-ui` authenticated clean-context preview/restricted browser story; configured auth/tenant remains Phase 14 | MUST pass agent story in browser | MUST keep migrated modules in same smoke matrix | MUST pass login/logout/auth reload/fresh profile plus desktop/narrow label QA without env toggles | MUST render diagnostics without console/network failures | MUST be required release smoke |
| Observability/audit | PASS existing audit gates preserved | PASS lifecycle projection reads existing version/release/ACL/grant audit sources; no new event type added | PASS native `business_events` cover successful release/rollback, `ctox.module.rollback_version`, failed release validation and failed rollback outcome; Settings/browser audit labels and redaction are Browser/Rust-proven | MUST audit audience/responsibility changes | MUST audit delegated agent decisions | MUST audit module exception decisions | MUST capture smoke diagnostics | MUST implement native/MCP retention, export, redaction and recovery | MUST document rollout/recovery |
| Runtime/storage safety | N/A core slice beyond guarded DB helper | N/A beyond projected lifecycle | PASS release smoke proves localStorage cannot decide release visibility/state for the App Store publish path; broader storage scoping remains Phase 13/14 | PASS audience smoke proves localStorage taskbar/fake-audience tampering cannot widen app visibility; broader storage scoping remains Phase 13/14 | MUST ensure agent context cannot be spoofed through stored client state | MUST define dynamic app runtime boundary and storage-scope guard | MUST prove fresh/copy/cleared storage does not widen access | MUST include storage/runtime facts in diagnostics | MUST be security/release signoff |
| Performance/scale | N/A | N/A | PASS release smoke duration and zero browser warning/error/request-failure budget for representative release fixture; catalog/audit scale budgets remain Phase 14 | MUST cover audience size | MUST cover agent grant/scope volume | MUST cover module migration inventory size | MUST enforce startup/sync/action budgets in browser matrix | MUST cover audit/diagnostic volume and retention | MUST be required release budget |
| Backup/restore | N/A | N/A | PASS targeted lifecycle projection repair for release rows/catalog; MUST still include operator backup/restore drill in Phase 15 | MUST recover orphan/private audience states | MUST preserve agent grants/audit evidence | MUST preserve module exception inventory | MUST reload restored state in browser | PARTIAL PASS native isolated `ctox business-os backup restore-drill`; MUST still cover active-root restore/quiesce, browser unsynced IndexedDB and WebRTC resync | MUST be documented and release-gated |
| Security/privacy | PASS no extra source/data grant through visible UI slice | PASS no more-public lifecycle backfill | MUST preserve source/release/data-review boundaries | PARTIAL PASS 11A/11B `apps.view` is separate from data/source/release/edit and hidden direct opens are blocked before import; MUST still cover management UI/responsibility privacy | MUST prevent agent context spoofing and hidden extra scope | MUST define runtime capability boundary | MUST cover auth/tenant/fresh-profile privacy | MUST redact support/audit exports | MUST block release without signoff |
| WebRTC/RxDB guardrails | PASS no HTTP fallback | PASS no new RxDB collection, no HTTP fallback, no dist patch, recoverable native catalog sync fallback | MUST route release/rollback via commands/WebRTC and never re-enable legacy HTTP handlers | PASS audience grants/state stay in native `business_permission_grants`/catalog projection and Browser/Rust smoke uses WebRTC/RxDB, no HTTP fallback or dist patch | MUST keep MCP separate from browser data path while sharing policy semantics | MUST avoid dist patch and generated-contract drift | MUST enforce browser health and no HTTP data bridge | MUST keep recovery WebRTC-only | MUST be enforced in CI |

## Cross-Phase Test Matrix

Each completed phase should update this table with pass/fail evidence.
Evidence must include the exact command, result, date and log/artifact link when
available. `N/A` means the area was not in scope for that phase. `TBD` is only
valid for legacy update-log entries or explicitly future evidence, not for
current phase blockers.

| Area | Phase 0 | Phase 1 | Phase 2 | Phase 3 | Phase 4 | Phase 5 | Phase 6 | Phase 7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Role normalization | PASS targeted + `business_os` gate | PASS policy + store tests | PASS via `cargo test --bin ctox business_os` | PASS `roles.test.mjs` plus `business_os` gate | PASS `roles.test.mjs` and Settings touched-label cleanup | PASS MCP service actors resolve persisted Business OS roles or exact synthetic-user grants | PASS broad `business_os` gate and rollout docs preserve `chef/admin/founder/user` storage plus business-facing labels | PASS live role smoke checks Owner/Admin/App-Verantwortliche:r/Teammitglied labels and rejects legacy raw wording |
| Policy matrix | PASS baseline role/naming contract | PASS Rust policy unit test | PASS existing policy regression | PASS Phase 3A permission catalog/projection plus Phase 3B grant-aware routing for module, selected workspace, task and App Store command families; Phase 3F accepts exact-grants-only object ownership for current rollout | PASS shared browser permission helper tests for role defaults, module assignments and explicit grants | PASS MCP shared-policy routing for module, collection, exact record, approval and MCP scopes; Phase 5H accepts automatic ownership derivation as deferred until normalized product semantics exist | PASS Phase 6 audit/Activity gates cover allowed and denied native policy decisions | PASS live role smoke verifies Teammitglied, exact source-view grant, exact modify grant, Owner defaults and exact module-scope isolation |
| Spoofed client role rejection | PASS targeted | PASS targeted | PASS targeted with app-build crafted command coverage | PASS covered by `cargo test --bin ctox business_os` regression suite | PASS UI helper consumes projected permissions instead of trusting local role labels | PASS unknown MCP actors do not inherit local bootstrap Admin rights | PASS broad `business_os` gate after Phase 6G | PASS smoke applies governance model in-browser and verifies scoped grants do not leak to `ctox` |
| App modify right-click path | N/A | Existing module modify wrapper now policy-backed; selected App Store target later fixed in Phase 4A | PASS backend app-build modify gate; selected App Store UI target still Phase 4 | PASS module-scoped explicit grant backend path; browser affordance still Phase 4 | PASS App Store selected-app context target, Shell right-click `App ändern` render tests and global CTOX context-mode tests | N/A for MCP channel | N/A for rollout/audit | PASS live Shell context menu shows `App ändern` only for Owner or exact modify grant on the scoped app |
| App source view | N/A | PASS policy matrix includes `apps.source.view` | PASS source save remains `apps.modify`; source-load gap closed in Phase 3E | PASS source load and source snapshot listing require grant-aware `apps.source.view`; exact module grants allow read-only source access | PASS Shell right-click, Appbar and Source Editor filter by projected `apps.source.view`; Source Editor is read-only without `apps.modify` and treats denied commands as hard errors | N/A for MCP channel | N/A for rollout/audit | PASS live context menu and Appbar expose Source only to Owner or exact source-view grant, not Teammitglied or modify-only grant |
| Module release/rollback | N/A | PASS via `cargo test --bin ctox business_os` | PASS structured deny catalog + native allow regression | PASS covered by `cargo test --bin ctox business_os` regression suite | Existing UI behavior not expanded in this rollout | N/A for MCP channel | N/A for rollout/audit | PASS broad `cargo test --bin ctox business_os` remains green after smoke additions |
| App install/uninstall | PASS App Store hook test via temp esbuild setup; backend Owner/Admin gate exists | PASS via `business_os` gate; explicit command hardening still Phase 2 | PASS structured Teammitglied deny plus Admin allow through hermetic App Store install/uninstall test | PASS explicit module install/uninstall grants plus `business_os` regression suite | PASS App Store action hooks require projected permissions plus metadata, expose disabled reasons on deny, recompute after grant refresh and render disabled Teammitglied reasons in static Playwright smoke | N/A for MCP channel | N/A for rollout/audit | PASS UI regression smoke still covers App Store shell and core module inventory while role smoke covers selected app permissions |
| User and role management | N/A | PASS policy-backed wrapper + `business_os` gate | PASS structured Teammitglied deny for user upsert | PASS exact workspace `users.manage` grant allows covered user-upsert command without global admin role; PASS Owner-transfer guard requires `workspace.manage` and denies Admin/grant attempts to assign `chef`/`owner` | PASS Settings render-label tests, Owner-only role-option helper, static UI permission guard and Owner/Teammitglied static Playwright smokes proving old touched labels are absent | N/A for MCP channel | PASS role changes emit queryable `business_events` rows with previous/current state and trusted actor context; Settings Activity tab exposes them to Owner/Admin | PASS live role smoke verifies Owner can see the Owner target role and Admin cannot |
| RxDB/WebRTC projection | PASS run-all; 2 wire-daemon skips | N/A for no schema change | N/A for no schema change | PASS existing `business_module_catalog.governance.permission_model` projection; no new collection in Phase 3A; `run-all.mjs` 37 passed, 0 failed, 2 wire-daemon skips | PASS shared browser helper consumes projection shape; `run-all.mjs` 37 passed, 0 failed, 2 skips | N/A for MCP-only native policy alignment; no RxDB schema added | PASS no new RxDB collection or HTTP data path; `run-all.mjs` 37 passed, 0 failed, 2 wire-daemon skips; `assert-rxdb-only.mjs` passed | PASS wire daemon built and `run-all.mjs` passed 39/39 with 0 skips; no HTTP fallback or new collection added |
| CTOX task manage | N/A | N/A | PASS structured Teammitglied deny plus authorized native task update regression | PASS exact task `ctox.task.manage` grant allows covered task update command | N/A for browser UI slice | N/A for MCP channel | N/A for rollout/audit | PASS broad `business_os` gate remains green |
| Structured denied-command output | N/A | N/A | PASS for app-build, task manage and main native control-command gates via `business_commands.result.policy_decision` | PASS regression via `control_commands_return_structured_policy_denials` after grant-aware routing | N/A for browser UI slice | N/A; MCP uses `business_os_mcp_events` instead | PASS native Activity/audit slices cover denied policy decisions | PASS broad `business_os` gate remains green; browser role smoke verifies denied routine affordances stay hidden |
| App Store selected-app modify semantics | N/A | N/A | N/A; selected-target UI fixed in Phase 4 | N/A; selected-target UI fixed in Phase 4 | PASS selected App Store context app-modify emits selected `record_id`, `payload.module_id/app_id` and `client_context.module_id/app_id` | N/A for MCP channel | N/A for rollout/audit | PASS live role smoke targets selected module `app-store` and verifies exact modify grant does not affect `ctox` |
| MCP policy alignment | N/A | N/A | N/A | N/A | N/A | PASS Phase 5A/5B/5C/5D/5E shared evaluator gates for `execute_action` (`data.write`), approval decision routing (`external.approve`), module detail/action-proposal reads, collection-backed record reads, exact record reads, exact approval grant routing, module listing, scoped links, command status, MCP status/activity and service actors; Phase 5F/5H source-validate and test that arbitrary record payloads and `actor_user_id` do not create implicit grants; Phase 5G accepts `business_os.approve` and external-effect execution as blocked for MCP Channel v1/current rollout | PASS broad `business_os` gate preserves MCP policy tests; MCP audit stays separate from native Activity | PASS broad `cargo test --bin ctox business_os` preserves MCP policy tests after Phase 7 |
| Audit events | N/A | N/A | N/A | N/A | N/A | PASS MCP audit metadata includes compatible `policy_decision` plus `resolved_actor` for shared-policy-covered MCP read/write/status/exact-scope decisions; general Business OS policy audit remains Phase 6 | PASS native denied and allowed policy decisions, role changes, app-responsibility changes and Outbound approval decisions insert queryable `business_events` rows with trusted actor context and relevant previous/current/decision details; PASS admin Activity UI queries those events through `ctox.business_os.audit.list`; allowed Activity-list self-events are intentionally skipped | PASS broad `business_os` gate preserves audit tests; no new audit store added |
| Rollout and recovery docs | N/A | N/A | N/A | N/A | N/A | PASS MCP v1 has separate release/security admin docs | PASS `docs/business-os-roles-permissions-rollout.md` documents role aliases, data stores, rollout checks, recovery boundaries and ship criteria; Phase 6G broad release gates passed | PASS plan documents local production-ready gate evidence and residual deployment boundary |

## Open Decisions

| Decision | Options | Current Recommendation | Status |
| --- | --- | --- | --- |
| Canonical stored Owner role | keep `chef`; migrate to `owner` | keep `chef` for this rollout; UI may say Owner, but storage remains `chef/admin/founder/user` and `owner` stays a compatibility alias | Accepted for current rollout |
| `team` alias | unsupported; normalize to `user`; migrate to stored `team` later | accepted in Phase 0 as explicit alias to canonical `user`, covered by Rust and JS tests | Accepted |
| UI label for `founder` | Founder; App Owner; App-Verantwortliche:r | App-Verantwortliche:r | Accepted |
| App source visibility for Team | hide; read-only grant; app-level option | default hidden; exact app-level `apps.source.view` grant allows read-only Source, while `apps.modify` is still required to save | Accepted for Phase 3E/4F |
| Admin ability to assign Owner | yes; no; only transfer flow | no, Owner-only transfer; Phase 3D/4E implemented backend `workspace.manage` enforcement and Owner-only Settings role options | Accepted for current rollout |
| Deny UI pattern | hide; disabled with reason; mixed | mixed: hide routine actions, disable high-value actions with reason; Phase 4C/4D source and tests validate this for Shell and App Store | Accepted for Phase 4E |
| Generic grants architecture | table only; RxDB collection; derived effective-permission projection | Phase 3A accepted: native allow-only table plus existing catalog `governance.permission_model`; add a new RxDB collection only if later UI/query requirements need it | Accepted for Phase 3A |
| App create/modify policy gate | Owner/Admin only; assigned App-Verantwortliche for target app; approval flow | app-create requires Owner/Admin; app-modify allows Owner/Admin or assigned App-Verantwortliche:r | Backend app-build command gate and Phase 4 selected-app UI semantics implemented |
| App Store selected-app modify | modify App Store itself; modify selected installed app; separate actions | selected app for App Store context app-modify; separate bulk/store actions can be added later if needed | Accepted for Phase 4A context menu |
| MCP service-account representation | new service role; new `subject_type=service_account`; existing Business OS user/grant identity | use existing actor ids as `business_users.user_id` or exact `business_permission_grants(subject_type=user, subject_id=<actor>)`; add alias/lifecycle metadata only if product needs it | Accepted for Phase 5D |
| MCP record scope id convention | raw record id; collection-qualified record id; separate structured key | use `scope_id=<collection>/<record_id>` for exact record grants so ids stay unambiguous across collections | Accepted for Phase 5E |
| Automatic record/approval ownership derivation | derive from record metadata; require exact grants; module/collection fallback only | keep exact record/approval grants plus current collection/module/outbound fallbacks for current rollout; implement ownership derivation only after a normalized product ownership contract defines authoritative fields and read/write/approval semantics | Accepted for Phase 3F/5H/current rollout |
| MCP external effects | always blocked; approval-gated; role/grant-gated | keep blocked for MCP Channel v1/current rollout; enabling approved external effects is a later product phase with explicit risk model and tests | Accepted for Phase 5G/current rollout |
| General policy audit scope | separate table; reuse `business_events`; unify with MCP audit | Phase 6A/6B/6D/6F accepted: native denied and allowed policy decisions, role changes, app-responsibility changes and Outbound approval decisions reuse existing `business_events`; Phase 6C exposes those native events through an admin-only `business_commands` query; MCP audit remains in `business_os_mcp_events`; future retention/volume policy can be added as a separate product decision | Accepted for Phase 6A/6B/6C/6D/6F |
| Durable app lifecycle storage | manifest fields; native table projected into catalog; new RxDB collection | Phase 9 implemented native persistence/projection through the existing `business_module_catalog` document and `governance.lifecycle`; no new RxDB collection was added | Accepted/implemented in Phase 9 |
| Release data-access review storage | store in release row; store in lifecycle metadata; separate review table | store immutable review snapshot with the release and project current summary to catalog; avoid mutable-only review state for audit/replay | Backend accepted/implemented through Phase 10E2 plus 10D2 repair/replay consistency; Shell/App Store/Settings static projection consumption exists and Settings fallback is read-only/browser-proven, but App Store publish wizard and Browser/Rust reload evidence remain required |
| Release data-access grant semantics | review creates grants; review references existing grants; review plus locked-state contract | review is evidence only; Phase 10B backend validates manifest read/write collections and reconciles explicit Team `data.read`/`data.write` grants or declared locked-state behavior without creating implicit grants | Backend accepted/implemented in Phase 10B; App Store/Browser rendering evidence remains in Phase 10 |
| Rollback permission contract | reuse `apps.modify`; use `apps.rollback`; use both | use `apps.rollback` end to end for release rollback and source rollback; `apps.modify` is not a substitute for rollback rights | Accepted for Phase 10 |
| Release/rollback consistency strategy | best-effort file writes; SQLite-only release rows; explicit consistency/repair path | 10D1 implemented tested stale-version preflight, transactional release/source-version SQLite writes and `module.json` restore on injected DB failures; 10D2 implemented duplicate release replay idempotency plus lifecycle projection repair for release rows/RxDB/catalog; 10E2 implemented happy-path current release/rollback/data-area projection and runtime-installed manifest-root consistency; Phase 15 must still close broader operator recovery, backup/restore and support recovery drills | Backend consistency accepted for Phase 10; operational recovery remains required before Phase 15/full production completion |
| Preview/restricted audience model | exact edit grants only; named audience table; role/user allowlist in lifecycle metadata; dedicated visibility grant/state | introduce explicit visibility semantics separate from `apps.modify`, `apps.source.view`, `apps.release` and data grants; add named audience state only when reusable groups/request workflows need it | Required before Phase 11 implementation |
| MCP app visibility vs data access | use data grants for module visibility; add app visibility before data; keep browser-only visibility | evaluate app lifecycle/audience visibility before MCP module listing, links, proposals and execution; data grants unlock data only after app visibility passes | Accepted/implemented in Phase 12A; browser AI scope panels, client-context integrity, read-only grant-boundary UX and native/MCP audit metadata parity are implemented in Phase 12B-12E |
| Agent app-scope management UI | Settings only; App Store detail; per-app AI controls; MCP admin surface | App Store detail owns durable app scopes; per-app AI controls show active actor/scope read-only; Settings remains workspace-wide admin surface | Implemented for Phase 12 as visible scope panels plus a read-only Owner/Admin grant-boundary surface; grant mutation remains future work until a server-authoritative command exists |
| Packaged/core DB isolation exceptions | migrate everything; allow broad system access; explicit allowlist | migrate user-facing packaged apps to guarded facade; keep explicit, documented system exceptions only where the module is core infrastructure, with committed inventory artifact | Required before Phase 13 implementation |
| Auth/tenant production smoke target | local-session only; configured auth user; managed ctox.dev; all deployment modes | local configured-auth smoke is the minimum release gate; managed/public deployment smoke is required before claiming hosted production readiness; no new env-var production toggles | Required before Phase 14/16 release gate |
| Audit retention and support export | unlimited events; fixed TTL; configurable retention; export-only pruning | configurable retention with support-safe export before prune; no body/secrets/tokens in exported diagnostics | Required before Phase 15 implementation |
| Release CI artifact gate | process exit only; fixed artifact; uploaded CI artifact with schema validation | fixed-path Browser/Rust smoke summary plus schema validation and release-workflow prerequisite job before artifacts upload | Required for Phase 16 completion |
| Dynamic app runtime trust boundary | same-origin trusted code; sandboxed iframe/worker; restricted capability API | define an explicit trusted-code boundary before claiming production-ready; if runtime apps stay same-origin trusted code, document that as a product/security boundary and still guard data/storage/network/external effects with tests | Required before Phase 13/16 completion |
| Browser storage authority | use local/session storage for lifecycle/permissions; use storage only for UI hints; move all state native | storage may keep UI/session hints only; roles, visibility, release, audience, tenant and data grants must come from native/RxDB projection or typed auth state | Required before Phase 13/14 completion |
| Performance/scale release budget | no explicit budget; smoke duration only; per-surface budgets | add representative app/grant/audit/release-history fixtures and enforce startup/sync/action budgets in the production smoke artifact | Required before Phase 14/16 completion |
| Backup/restore boundary | manual only; native command; documented operator drill | CLI-only native isolated restore drill is implemented and tested; active-root restore is represented by a tested machine-readable manual runbook/preflight with quiesce/restart, manifest hash/signature, portable-export/key-escrow and compatibility gates. Local same-profile browser unsynced-state/WebRTC peer-restart proof is covered by `business-os-restore-resync-ui`. Raw manifests are signed and carry local retention/support-attachment policy, plus a verified AES-256-GCM portable encrypted export. Production-ready still requires hosted/multi-workspace restore proof before hosted claims, release-level cross-version/downgrade evidence and external key-escrow operational signoff | Partial for Phase 15; remaining required before full production-ready claim |
| Security/privacy release signoff | informal review; PR review only; blocking release artifact | add a blocking release checklist/artifact for dynamic app runtime, source visibility, data review, MCP/agent scopes, audit export, external effects and artifact integrity | Required before Phase 16 completion |

## Update Log

Historical update-log rows preserve the status at the time of that checkpoint.
When a later row closes a previously open item, the later row and the Phase
Tracker are authoritative for current status.

| Date | Update | Evidence |
| --- | --- | --- |
| 2026-06-16 | Initial plan created. | TBD |
| 2026-06-16 | Plan source-validated by read-only backend, RxDB/MCP, UI and coherence subagents; unsupported assumptions marked as new work or open decisions. | Subagent reports in thread |
| 2026-06-16 | Phase 0 implementation started: shared browser role helper added, backend `team` aliases normalize to `user`, targeted tests added and run. | See Phase 0 evidence |
| 2026-06-16 | Phase 0 gates re-run after source-aligned test fixes; UI/test-gate subagents verified remaining plan gaps around App Store selected-app modification, role-gated affordances, Settings labels and App Store test dependency setup. | `cargo test --bin ctox business_os`; `node src/apps/business-os/modules/app-store/app-store.test.mjs` via temp esbuild; subagent reports in thread |
| 2026-06-16 | Phase 1 implementation added central Rust policy evaluator and routed existing role/module authorization wrappers through it without UI behavior changes. | `cargo test --bin ctox business_os_policy`; `cargo test --bin ctox rxdb_command_auth_uses_trusted_user_role_not_client_claims`; `cargo test --bin ctox business_os`; `node src/apps/business-os/shared/roles.test.mjs` |
| 2026-06-16 | Phase 2 first hardening slice implemented for app-build command acceptance: app-create/app-modify and app-shaped commands are policy-gated before queue task creation, denied commands write structured `result.policy_decision`, and no queue task is created on deny. | `cargo test --bin ctox app_build_commands_enforce_policy_before_queueing`; `cargo test --bin ctox app_create_rxdb_command_accepts_type_alias_from_cli_dispatch`; `cargo test --bin ctox business_os_policy`; `cargo test --bin ctox rxdb_command_auth_uses_trusted_user_role_not_client_claims`; `cargo test --bin ctox business_os` |
| 2026-06-16 | Phase 2 control-command hardening slice implemented: native control commands for users, runtime, integrations/channels, source/module governance and App Store install/uninstall now use the shared structured policy-denial helper before side effects. | `cargo test --bin ctox control_commands_return_structured_policy_denials`; `cargo test --bin ctox native_peer_consumes_pending_module_governance_commands`; `cargo test --bin ctox app_create_rxdb_command_accepts_type_alias_from_cli_dispatch`; `cargo test --bin ctox business_os` |
| 2026-06-16 | Phase 2 backend enforcement completed for review: task update/delete now return structured task-manage denials, App Store install/uninstall has hermetic Admin allow coverage, and the full Business OS Rust gate passes. | `cargo test --bin ctox control_commands_return_structured_policy_denials`; `cargo test --bin ctox native_peer_consumes_pending_ctox_task_update_command`; `cargo test --bin ctox app_store_install_uninstall_allows_admin_policy_path`; `cargo test --bin ctox business_os` |
| 2026-06-16 | Phase 3A backend-only grants/projection slice added after read-only subagent validation: native allow-only `business_permission_grants`, grant-aware module policy decisions, and existing catalog `governance.permission_model` projection. | `cargo test --bin ctox permission_grant_allows_scoped_module_action_without_new_role`; `cargo test --bin ctox module_governance_projection_includes_permission_model_and_grants`; `cargo test --bin ctox business_os`; `node src/apps/business-os/rxdb/tests/run-all.mjs`; subagent reports in thread |
| 2026-06-16 | Phase 3B grant-routing slice added: selected workspace commands, CTOX task update/delete, template/founder actions and App Store install/uninstall now use grant-aware policy decisions and inner side-effect guards. | `cargo test --bin ctox workspace_permission_grant_allows_user_upsert_without_admin_role`; `cargo test --bin ctox task_permission_grant_allows_ctox_task_update_without_admin_role`; `cargo test --bin ctox app_store_install_uninstall_allows_explicit_module_grants`; `cargo test --bin ctox control_commands_return_structured_policy_denials`; `cargo test --bin ctox business_os`; `cargo test --manifest-path src/core/rxdb/Cargo.toml`; `node src/apps/business-os/rxdb/tests/run-all.mjs`; `cargo check` |
| 2026-06-16 | Phase 4A browser-permission slice added: shared permission helper consumes the Phase-3 projection, Shell/Settings/App Store affordances use it, touched Settings labels use business-facing wording, and App Store context modify targets the selected app. | `node src/apps/business-os/shared/permissions.test.mjs`; `node src/apps/business-os/shared/roles.test.mjs`; `node src/apps/business-os/modules/app-store/app-store.test.mjs` via temp esbuild symlink; `node src/apps/business-os/scripts/assert-module-conformance.mjs`; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `node src/apps/business-os/rxdb/tests/run-all.mjs` |
| 2026-06-16 | Phase 4B UI guard slice added: Settings template test covers business-facing labels, role badge no longer renders raw stored role values, and a static guard blocks new local UI role matrices or old Settings wording in Shell/Settings/App Store. | `node src/apps/business-os/shared/react-settings.test.mjs`; `node src/apps/business-os/scripts/assert-permissions-ui.mjs` |
| 2026-06-16 | Phase 4C App Store reason/refresh slice added: install/update/uninstall actions show disabled, plain-language denial reasons when relevant but not permitted, while edit remains hidden for unauthorized actors; App Store hooks prove affordances recompute after governance grants refresh. | `node src/apps/business-os/modules/app-store/app-store.test.mjs` via temp esbuild symlink |
| 2026-06-16 | Phase 4 browser-smoke evidence added: static App Store shell load succeeds with URL-packed pairing config, Owner badge renders with business-facing label, and old touched Settings labels are absent from visible text. | `/Users/michaelwelsch/.codex/skills/playwright/scripts/playwright_cli.sh open`; `/Users/michaelwelsch/.codex/skills/playwright/scripts/playwright_cli.sh snapshot`; `/Users/michaelwelsch/.codex/skills/playwright/scripts/playwright_cli.sh eval` |
| 2026-06-16 | Phase 4D shell right-click render slice added: Shell module context items and global CTOX context modes now render `App ändern` only when projected permissions allow app modification, old app-modification wording is statically guarded, and Teammitglied browser smoke proves disabled App Store reasons render in the real shell. | `node src/apps/business-os/shared/shell-permissions-ui.test.mjs`; `node src/apps/business-os/scripts/assert-permissions-ui.mjs`; `node src/apps/business-os/modules/app-store/app-store.test.mjs` via temp esbuild symlink; `/Users/michaelwelsch/.codex/skills/playwright/scripts/playwright_cli.sh eval` |
| 2026-06-16 | Phase 5A MCP shared-policy slice added: MCP action execution and approval decision routing now use the shared Business OS evaluator while existing MCP allowlists remain stricter filters; covered MCP audit events include `policy_decision`. | `cargo test --bin ctox mcp_business_os_policy`; `cargo test --bin ctox audited_mcp_policy_denial_records_business_os_policy_decision`; `cargo test --bin ctox execute_action_blocks_external_effects_even_when_approved`; `cargo test --bin ctox approval_decision_enqueues_typed_outbound_command`; `cargo test --bin ctox request_changes_enqueues_typed_outbound_command`; `cargo test --quiet --bin ctox business_os::mcp_channel::tests`; `cargo test --quiet --bin ctox business_os::policy` |
| 2026-06-16 | Phase 5B MCP read-policy slice added: module detail/action-proposal reads and collection-backed record reads now route through the shared Business OS evaluator, with collection reads allowed by exact collection grant or mapped module `data.read` grant. | `cargo test --bin ctox mcp_business_os_policy`; `cargo test --bin ctox audited_mcp_read_denial_records_business_os_policy_decision`; `cargo test --quiet --bin ctox business_os::mcp_channel::tests`; `cargo test --quiet --bin ctox business_os::policy` |
| 2026-06-17 | Phase 5C MCP coverage slice added: module listings are shared-policy filtered, scoped deep links and command status now use shared read policy, MCP status/activity now require shared `mcp.manage`, and action execution checks `data.write` before proposal lookup. | `cargo test --bin ctox mcp_business_os_policy`; `cargo test --quiet --bin ctox business_os::mcp_channel::tests`; `cargo test --quiet --bin ctox business_os::policy`; `rustfmt --edition 2021 --check src/core/business_os/mcp_channel.rs src/core/business_os/store.rs` |
| 2026-06-17 | Phase 5D service-actor mapping slice added: MCP actors resolve through persisted `business_users` when present, otherwise as synthetic `user` service actors that require exact grants; unknown MCP actors no longer inherit local bootstrap Admin rights; MCP command contexts and audit metadata now include resolved Business OS actor identity. | `cargo test --bin ctox mcp_business_os_policy`; `cargo test --quiet --bin ctox business_os::mcp_channel::tests`; `cargo test --quiet --bin ctox business_os::policy`; `rustfmt --edition 2021 --check src/core/business_os/mcp_channel.rs src/core/business_os/store.rs`; subagent reports in thread |
| 2026-06-17 | Phase 5E exact-scope MCP slice added: exact record grants authorize only targeted `business_os.get_record`, exact approval grants authorize targeted approval decision routing without a broad `outbound` module grant, and aggregate record reads remain collection/module-gated. | `cargo test --bin ctox mcp_business_os_policy`; `cargo test --quiet --bin ctox business_os::mcp_channel::tests`; `cargo test --quiet --bin ctox business_os::policy`; `rustfmt --edition 2021 --check src/core/business_os/mcp_channel.rs src/core/business_os/store.rs`; `git diff --check -- src/core/business_os/mcp_channel.rs src/core/business_os/store.rs` |
| 2026-06-17 | Phase 6A native policy-denial audit slice added: denied native Business OS policy decisions now write queryable `business_events` rows with trusted actor context and the shared `policy_decision`, while the existing `business_commands.result.policy_decision` payload remains unchanged. | `cargo test --bin ctox policy_denial_writes_business_event_audit`; `cargo test --bin ctox control_commands_return_structured_policy_denials`; `cargo test --bin ctox app_build_commands_enforce_policy_before_queueing`; `cargo test --quiet --bin ctox business_os::policy`; `rustfmt --edition 2021 --check src/core/business_os/store.rs src/core/business_os/mcp_channel.rs`; `git diff --check -- src/core/business_os/store.rs src/core/business_os/mcp_channel.rs`; `! grep -n '[[:blank:]]$' docs/business-os-roles-permissions-plan.md` |
| 2026-06-17 | Phase 6B role/app-responsibility audit slice added: native user changes and app-responsibility assignments now write queryable `business_events` rows with trusted actor context, previous/current state and changed fields. | `cargo test --bin ctox user_role_change_writes_business_event_audit`; `cargo test --bin ctox module_founder_assignment_writes_business_event_audit`; `cargo test --bin ctox workspace_permission_grant_allows_user_upsert_without_admin_role`; `cargo test --bin ctox native_peer_consumes_pending_module_governance_commands`; `cargo test --bin ctox policy_denial_writes_business_event_audit` |
| 2026-06-17 | Phase 6C Activity slice added: Owner/Admin Settings now has an Activity tab backed by the `ctox.business_os.audit.list` control command over existing native `business_events`, with no new RxDB collection or HTTP data path. | `cargo test --bin ctox audit_list_command`; `cargo test --bin ctox business_event_audit`; `node src/apps/business-os/shared/react-settings.test.mjs`; `node --check src/apps/business-os/shared/react-settings.js`; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `rustfmt --edition 2021 --check src/core/business_os/store.rs`; `git diff --check -- src/core/business_os/store.rs src/apps/business-os/shared/react-settings.js src/apps/business-os/shared/react-settings.test.mjs docs/business-os-roles-permissions-plan.md`; subagent report in thread |
| 2026-06-17 | Phase 6D external approval activity slice added after source validation: native Outbound approval decisions are recorded in existing `business_events` and surfaced through the existing Activity tab, without a new RxDB collection or HTTP data path. | `cargo test --bin ctox outbound_approval_decisions_write_business_event_audit`; `cargo test --bin ctox business_event_audit`; `node src/apps/business-os/shared/react-settings.test.mjs`; `node --check src/apps/business-os/shared/react-settings.js`; `rustfmt --edition 2021 --check src/core/business_os/store.rs` |
| 2026-06-17 | Phase 6E rollout/recovery documentation slice added: operator guidance now documents role aliases, native data stores, Activity coverage, release checks, recovery boundaries and ship criteria for the roles/permissions rollout. | `test -s docs/business-os-roles-permissions-rollout.md`; `rg -n "Business OS Roles and Permissions Rollout Guide|Recovery|Ship Criteria" docs/business-os-roles-permissions-rollout.md`; `! grep -n '[[:blank:]]$' docs/business-os-roles-permissions-rollout.md docs/business-os-roles-permissions-plan.md` |
| 2026-06-17 | Phase 6F allowed policy-decision audit slice added: allowed decisions from existing native policy gates are recorded as `business_os.policy.allowed` in existing `business_events` and surfaced through the existing Activity tab; allowed Activity-list self-events are skipped. | `cargo test --bin ctox allowed_policy_decision_writes_business_event_audit`; `cargo test --bin ctox business_event_audit`; `cargo test --bin ctox audit_list_command`; `node src/apps/business-os/shared/react-settings.test.mjs`; `node --check src/apps/business-os/shared/react-settings.js`; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `rustfmt --edition 2021 --check src/core/business_os/store.rs`; `git diff --check -- src/core/business_os/store.rs src/apps/business-os/shared/react-settings.js`; `! grep -n '[[:blank:]]$' docs/business-os-roles-permissions-plan.md docs/business-os-roles-permissions-rollout.md src/apps/business-os/shared/react-settings.test.mjs` |
| 2026-06-17 | Phase 6G plan-coherence and release-gate slice added after read-only subagent validation: stale tracker/matrix/open-decision language was corrected, denied Activity-list audit persistence got a direct regression assertion, and Phase 6 moved to Ready for review. | `cargo test --bin ctox audit_list_command`; `cargo test --bin ctox business_event_audit`; `cargo test --bin ctox business_os`; `cargo test --manifest-path src/core/rxdb/Cargo.toml`; `node src/apps/business-os/rxdb/tests/run-all.mjs`; `node src/apps/business-os/shared/react-settings.test.mjs`; `node --check src/apps/business-os/shared/react-settings.js`; `node src/apps/business-os/shared/roles.test.mjs`; `node src/apps/business-os/shared/permissions.test.mjs`; `node src/apps/business-os/shared/shell-permissions-ui.test.mjs`; `node src/apps/business-os/scripts/assert-permissions-ui.mjs`; `rustfmt --edition 2021 --check src/core/business_os/store.rs`; read-only subagent reports Poincare and Socrates |
| 2026-06-17 | Phase 5F decision-boundary slice added: the plan now explicitly distinguishes implemented MCP shared-policy gates from product-open ownership derivation and external-effect execution semantics; Gibbs corrected the `business_os.approve` wording to reflect the outer external-effect MCP block, and Euler's exact-approval negative test gap was closed. | Source validation against `src/core/business_os/store.rs`, `src/core/business_os/policy.rs` and `src/core/business_os/mcp_channel.rs`; `cargo test --bin ctox execute_action_blocks_external_effects_even_when_approved`; `cargo test --bin ctox mcp_policy_blocks_external_effect_approval_by_default`; `cargo test --bin ctox mcp_business_os_policy_denies_other_approval_for_exact_approval_grant`; `cargo test --bin ctox mcp_business_os_policy`; `cargo test --quiet --bin ctox business_os::mcp_channel::tests`; `cargo test --quiet --bin ctox business_os::policy`; `rustfmt --edition 2021 --check src/core/business_os/mcp_channel.rs`; `git diff --check -- docs/business-os-roles-permissions-plan.md src/core/business_os/mcp_channel.rs`; read-only subagents Gibbs and Euler |
| 2026-06-17 | Phase 5G MCP external-effect boundary accepted for current rollout: MCP Channel v1 keeps external effects blocked by default and `execute_action` still blocks them after shared policy and confirmation; enabling approved external effects is deferred. | Source validation against `src/core/business_os/mcp_channel.rs`; `cargo test --bin ctox mcp_policy_blocks_external_effect_approval_by_default`; `cargo test --bin ctox execute_action_blocks_external_effects_even_when_approved`; `cargo test --bin ctox approval_decision_enqueues_typed_outbound_command`; `cargo test --bin ctox request_changes_enqueues_typed_outbound_command`; `! grep -n '[[:blank:]]$' docs/business-os-roles-permissions-plan.md` |
| 2026-06-17 | Phase 3C source-validation boundary added: CTOX task ownership derivation is not implemented today; `lease_owner` is runtime routing state, while exact task grants remain the implemented non-admin path. | Source validation against `src/core/business_os/policy.rs`, `src/core/business_os/store.rs` and `src/core/mission/channels.rs`; `cargo test --bin ctox task_permission_grant_allows_ctox_task_update_without_admin_role` |
| 2026-06-17 | Phase 3D/4E Owner-transfer boundary added after read-only backend and frontend subagent validation: Admins and exact `users.manage` grants can manage users but cannot assign `chef`/`owner`; Settings hides the Owner target role for Admin while Owner retains it. | Read-only subagents Carver and Franklin; `cargo test --bin ctox owner_role`; `node src/apps/business-os/shared/roles.test.mjs`; `node src/apps/business-os/shared/react-settings.test.mjs`; `node src/apps/business-os/scripts/assert-permissions-ui.mjs` |
| 2026-06-17 | Phase 4E Deny UI decision accepted after source validation: routine actions are hidden, high-value App Store actions stay disabled with plain reasons, and no code change was required beyond plan updates. | Read-only subagent Galileo; `node src/apps/business-os/shared/shell-permissions-ui.test.mjs`; `node src/apps/business-os/modules/app-store/app-store.test.mjs` via temporary module-local esbuild symlink; `node src/apps/business-os/scripts/assert-permissions-ui.mjs` |
| 2026-06-17 | Phase 3E/4F App source visibility slice added after read-only subagent validation: `ctox.source.load` and `ctox.source.list_snapshots` now require `apps.source.view`; Shell right-click, module Appbar and Source Editor consume the same projected source-view model; Source Editor is read-only without `apps.modify` and treats denied source commands as hard errors. | Read-only subagent Anscombe; `cargo test --bin ctox source_load_requires_source_view_permission`; `node src/apps/business-os/shared/permissions.test.mjs`; `node src/apps/business-os/shared/shell-permissions-ui.test.mjs`; `node src/apps/business-os/desktop-apps/code-editor/code-editor.test.mjs` via temporary module-local esbuild symlink; `node src/apps/business-os/scripts/assert-permissions-ui.mjs`; `node --check src/apps/business-os/app.js src/apps/business-os/desktop-apps/code-editor/app.js src/apps/business-os/shared/shell-permissions-ui.js`; `rustfmt --edition 2021 --check src/core/business_os/store.rs` |
| 2026-06-17 | Phase 3F/5H exact-grants-only ownership boundary accepted after read-only native and MCP subagent validation: arbitrary record payload ownership fields and `outbound_approvals.actor_user_id` do not create implicit grants; Phase 3 and Phase 5 moved to Ready for review. | Read-only subagents Archimedes and Pauli; `cargo test --bin ctox record_owner_payload_field_does_not_grant_native_policy_access`; `cargo test --bin ctox mcp_approval_actor_user_id_does_not_replace_exact_approval_grant`; `cargo test --bin ctox mcp_record_owner_payload_field_does_not_replace_exact_record_grant`; `cargo test --bin ctox mcp_business_os_policy`; `rustfmt --edition 2021 --check src/core/business_os/store.rs src/core/business_os/mcp_channel.rs`; `git diff --check -- src/core/business_os/store.rs src/core/business_os/mcp_channel.rs docs/business-os-roles-permissions-plan.md`; `! grep -n '[[:blank:]]$' src/core/business_os/store.rs src/core/business_os/mcp_channel.rs docs/business-os-roles-permissions-plan.md` |
| 2026-06-17 | Completion audit/native-peer Owner-transfer alignment: broad `business_os` gate exposed a stale native-peer governance expectation; the test now asserts Admin owner-upsert denial and persisted Owner success, and the release gates were rerun against the updated source. | `cargo test --bin ctox native_peer_consumes_pending_module_governance_commands`; `cargo test --bin ctox business_os`; `cargo test --manifest-path src/core/rxdb/Cargo.toml`; `node src/apps/business-os/rxdb/tests/run-all.mjs`; `node src/apps/business-os/shared/roles.test.mjs`; `node src/apps/business-os/shared/permissions.test.mjs`; `node src/apps/business-os/shared/shell-permissions-ui.test.mjs`; `node src/apps/business-os/shared/react-settings.test.mjs`; `node src/apps/business-os/modules/app-store/app-store.test.mjs` via temporary module-local `esbuild` symlink, removed after the test; `node src/apps/business-os/desktop-apps/code-editor/code-editor.test.mjs` via the same temporary symlink; `node src/apps/business-os/scripts/assert-permissions-ui.mjs`; `node --check src/apps/business-os/app.js src/apps/business-os/shared/react-settings.js src/apps/business-os/desktop-apps/code-editor/app.js src/apps/business-os/shared/shell-permissions-ui.js`; `cargo check`; `rustfmt --edition 2021 --check src/core/business_os/rxdb_peer.rs src/core/business_os/store.rs src/core/business_os/mcp_channel.rs`; whitespace and diff hygiene checks |
| 2026-06-17 | Final completion audit: all current Phase Tracker rows and phase evidence blocks moved to `Complete`; the previously missing RxDB format guard was fixed mechanically and rerun with the canonical RxDB guard/test suite. | `cargo fmt --manifest-path src/core/rxdb/Cargo.toml`; `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`; `cargo test --manifest-path src/core/rxdb/Cargo.toml` — 239 unit tests and 30 conformance tests passed; `node src/apps/business-os/rxdb/tests/run-all.mjs` — 37 passed, 0 failed, 2 wire-daemon skips; `cargo test --bin ctox business_os` — 239 passed, 0 failed; final plan/status, whitespace and diff hygiene checks |
| 2026-06-17 | Phase 7 production-readiness hardening completed: live Browser/Rust role-permission smoke added, smoke matrix evidence hardened, the combined UI-regression plus role-permission matrix passed, wire-daemon skips were eliminated, and broad RxDB/Rust/JS release gates passed locally. | `node --check src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js src/apps/business-os/app.js src/apps/business-os/shared/permissions.js src/apps/business-os/shared/roles.js src/apps/business-os/shared/shell-permissions-ui.js`; `node src/apps/business-os/shared/roles.test.mjs && node src/apps/business-os/shared/permissions.test.mjs && node src/apps/business-os/shared/shell-permissions-ui.test.mjs && node src/apps/business-os/scripts/assert-permissions-ui.mjs`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-roles-permissions-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-ui-regression,business-os-roles-permissions-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/cargo-target cargo build --release --example v15_wire_daemon`; `node src/apps/business-os/rxdb/tests/run-all.mjs` - 39 passed, 0 failed, 0 skipped; `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`; `cargo test --manifest-path src/core/rxdb/Cargo.toml` - 239 unit tests and 30 conformance tests passed; `cargo test --bin ctox business_os` - 241 passed, 0 failed |
| 2026-06-17 | Phase 8 dynamic app lifecycle concept added: current `0.x` private / `1.0.0+` team visibility behavior was source-validated, the product model for icon badges, version display, visibility editing, app governance, data-access review and AI/agent scopes was documented, and the missing hard browser `ctx.db` data-isolation work was made explicit. | Source validation against `src/apps/business-os/app.js`, `src/apps/business-os/modules/app-store/index.js`, `src/apps/business-os/shared/permissions.js`, `src/core/business_os/policy.rs`, `src/core/business_os/store.rs`, `src/core/business_os/mcp_channel.rs`; concept in `docs/business-os-dynamic-apps-permissions-concept.md`; Phase Tracker row 8 added as `Not started` |
| 2026-06-17 | Phase 8 core implementation completed locally: dynamic app lifecycle logic is shared by Shell and App Store, runtime-installed `0.x` apps stay private for Team, `1.0.0+` apps are Team-default, restricted released apps are hidden from Team, invalid versions stay private, Shell/Appbar/App-Store badges and Lifecycle drawer render business-facing state, and runtime-installed module `ctx.db` access is guarded for `collection()`, collection properties and `raw`. | `node --check src/apps/business-os/shared/app-lifecycle.js src/apps/business-os/app.js src/apps/business-os/modules/app-store/index.js src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/shared/app-lifecycle.test.mjs && node src/apps/business-os/shared/permissions.test.mjs && node src/apps/business-os/shared/shell-permissions-ui.test.mjs`; `node src/apps/business-os/shared/roles.test.mjs && node src/apps/business-os/shared/react-settings.test.mjs`; `node src/apps/business-os/modules/app-store/app-store.test.mjs`; `node src/apps/business-os/scripts/assert-permissions-ui.mjs`; `node src/apps/business-os/scripts/assert-module-conformance.mjs`; `CARGO_TARGET_DIR=runtime/build/core-rxdb-integration-target cargo build --bin ctox`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=240000 BUSINESS_PORT=18989 SIGNALING_PORT=28989 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-roles-permissions-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=240000 BUSINESS_PORT=18988 SIGNALING_PORT=28988 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` |
| 2026-06-17 | Full production-readiness plan extension added: Phase 8 is marked complete for the verified core slice, and the production-readiness plan covers native lifecycle projection, App Store publish/data review, audience/preview management, agent-scope parity, packaged-module DB isolation, auth/tenant/fresh-profile browser E2E, observability/recovery and CI/release runbooks. | Source validation against `README.md`, `HARNESS.md`, `CLAUDE.md`, `docs/architecture.md`, `docs/ctox-rxdb.md`, `src/core/business_os/store.rs`, `src/core/business_os/server.rs`, `src/core/business_os/rxdb_peer.rs`, `src/apps/business-os/app.js`, `src/apps/business-os/modules/app-store/index.js`, `src/apps/business-os/shared/react-settings.js`, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js`; docs-only update |
| 2026-06-17 | Phase 9 native lifecycle projection completed: lifecycle metadata is projected natively into `business_module_catalog.governance.lifecycle`, installed app backfill is conservative for invalid/missing SemVer and `0.x`, projected SemVer overrides stale manifest versions in the browser, and recoverable native catalog sync conflicts use the existing repair/upsert fallback. | `rustfmt --edition 2021 src/core/business_os/rxdb_peer.rs src/core/business_os/store.rs --check`; `node --test src/apps/business-os/shared/app-lifecycle.test.mjs` - 9 passed; `node --check src/apps/business-os/shared/app-lifecycle.js`; `cargo test --bin ctox business_app_semver_major_matches_browser_plain_semver_contract` - 1 passed; `cargo test --bin ctox module_catalog` - 5 passed; `cargo test --bin ctox business_os` - 243 passed; `cargo build --bin ctox`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright CTOX_BIN=runtime/build/cargo-target/debug/ctox SMOKE_MODE=business-os-dynamic-apps-ui SMOKE_PAGE_PATH=/index.html /Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node src/core/rxdb/tools/browser_rust_smoke.js` - dynamic lifecycle reload smoke passed with 0 browser warnings, 0 browser errors, 0 request failures and 0 asset response errors; final diff hygiene gate before handoff |
| 2026-06-17 | Production-ready plan hardened after read-only subagent validation: Phase 10 now tracks rollback `apps.rollback` end-to-end enforcement, failed rollback outcomes, release/rollback consistency, App Store release wizard/test hooks and Settings fallback alignment; Phase 11-16 now include explicit visibility semantics, agent-scope UI, real Shell DB-facade isolation, registered Browser/Rust smoke modes, diagnostics/recovery, CI smoke artifacts and no-env-toggle auth/tenant gates. | Read-only subagent validation against `src/core/business_os/store.rs`, `src/core/business_os/policy.rs`, `src/core/business_os/server.rs`, `src/apps/business-os/app.js`, `src/apps/business-os/modules/app-store/index.js`, `src/apps/business-os/modules/app-store/index.css`, `src/apps/business-os/modules/app-store/app-store.test.mjs`, `src/apps/business-os/shared/app-lifecycle.js`, `src/apps/business-os/shared/permissions.js`, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js`; docs-only update |
| 2026-06-17 | Production-ready plan extended with operating model, blocker ledger, per-phase exit template and additional source-validated blockers: `ctox.module.list_versions` backend gate, release data-review grant reconciliation, no legacy HTTP release/source/rollback re-enable, MCP app visibility before data access, lifecycle badge/drawer permission UX, App Store detail control-plane sections, real runtime app DB-isolation smoke, visual label QA, native/MCP retention/export contract, release workflow gate, fixed smoke artifact, committed legacy migration fixtures and clean-checkout JS bootstrap. | Read-only Explorer validation against backend/policy/data-plane, UI/UX and test/CI/operations surfaces; docs-only update |
| 2026-06-17 | Phase 10A backend command-hardening slice implemented and reflected in the blocker ledger: release/source rollback helpers now require `apps.rollback`, rollback/release failures persist failed command outcomes, `ctox.module.list_versions` is backend-gated by source/release/rollback rights, and data-access review stores manifest-subset evidence with no implied grants. At this slice, grant reconciliation/locked-state still required Phase 10B; release/rollback consistency, App Store wizard, Settings fallback, no-HTTP guards, browser smoke and CI gates also remained open. | `rustfmt --edition 2021 src/core/business_os/store.rs --check`; `cargo test --bin ctox module_ -- --nocapture` - 31 passed, 0 failed; `cargo check --bin ctox` was aborted after extended dependency compile with no new error output and is not counted as passed evidence |
| 2026-06-17 | Phase 10B data-review reconciliation backend slice implemented: Team release reviews now require explicit Team `data.read`/`data.write` grants on the module or collection, or an explicit locked-state behavior per reviewed read/write collection. Release snapshots persist granted/locked reconciliation evidence and still do not create grants. At that point Phase 10 still had no-HTTP guards open; those were later closed in Phase 10C. | `rustfmt --edition 2021 src/core/business_os/store.rs --check`; `git diff --check -- src/core/business_os/store.rs`; `cargo test --bin ctox module_release_ -- --nocapture` - 5 passed, 0 failed; `cargo test --bin ctox module_ -- --nocapture` - 33 passed, 0 failed |
| 2026-06-17 | Production-ready plan expanded to cover all currently source-validated remaining elements: minimum production user stories, gate types, granular implementation slices 10C-16E, additional blocker-ledger rows for release projection/audit, Settings fallback, durable audience state, orphan prevention, deep links, agent client-context integrity, module inventory/raw DB lint, smoke mode registry, tenant boundary, diagnostics/recovery and release-gate enforcement. | Source validation against `src/core/business_os/server.rs`, `src/core/business_os/store.rs`, `src/core/business_os/rxdb_peer.rs`, `src/apps/business-os/app.js`, `src/apps/business-os/modules/app-store/index.js`, `src/apps/business-os/shared/react-settings.js`, `src/apps/business-os/shared/app-lifecycle.js`, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js`, `.github/workflows/ci.yml` and `.github/workflows/release.yml`; docs-only update |
| 2026-06-17 | Phase 10C no-legacy-HTTP slice implemented: direct `server.rs` route arms and helper code for module source load/save, module release and module rollback were removed; the RxDB-only guard now fails if those route strings, DTOs, helpers or direct release/rollback store markers return. Phase 10 remains in progress for release consistency, projection/audit, App Store wizard, Settings fallback and Browser/Rust release smoke. | `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `node --check src/apps/business-os/scripts/assert-rxdb-only.mjs`; `rustfmt --edition 2021 src/core/business_os/server.rs --check`; `cargo check --bin ctox`; `cargo test --bin ctox module_ -- --nocapture` - 33 passed, 0 failed; `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`; `cargo test --manifest-path src/core/rxdb/Cargo.toml` - 239 unit tests and 30 conformance tests passed; `node src/apps/business-os/rxdb/tests/run-all.mjs` - 37 passed, 0 failed, 2 skipped |
| 2026-06-17 | Phase 10D1 backend consistency sub-slice implemented and the production plan split 10D into 10D1/10D2: release now rejects stale `source_version_id`/`rollback_version_id` before `module.json` writes, synchronises release rows and manual source-version summaries in one SQLite transaction, and restores `module.json` on injected release/rollback DB failures. Historical status note: before the later 10D2 closure, the remaining backend consistency work covered projection-sync failure handling, duplicate command behavior and catalog repair/diagnostics. | `rustfmt --edition 2021 src/core/business_os/store.rs --check`; `cargo test --bin ctox module_release_ -- --nocapture` - 8 passed, 0 failed; `cargo test --bin ctox module_ -- --nocapture` - 36 passed, 0 failed; `cargo check --bin ctox`; `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`; `cargo test --manifest-path src/core/rxdb/Cargo.toml` - 239 unit tests and 30 conformance tests passed; `node src/apps/business-os/rxdb/tests/run-all.mjs` - 37 passed, 0 failed, 2 skipped |
| 2026-06-17 | Phase 10E1 native audit sub-slice implemented: successful release, successful rollback, failed release validation and failed rollback outcome now write queryable `business_events` with redacted business-facing summaries, and `ctox.business_os.audit.list` includes those event types. Historical checkpoint: Settings Activity/browser evidence was still open here and is closed by the later `P10-BE-AUDIT` row. | `rustfmt --edition 2021 src/core/business_os/store.rs --check`; `cargo test --bin ctox module_release_ -- --nocapture` - 11 passed, 0 failed; `cargo test --bin ctox module_ -- --nocapture` - 39 passed, 0 failed; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `cargo check --bin ctox`; `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`; `cargo test --manifest-path src/core/rxdb/Cargo.toml` - 239 unit tests and 30 conformance tests passed; `node src/apps/business-os/rxdb/tests/run-all.mjs` - 37 passed, 0 failed, 2 skipped |
| 2026-06-17 | Phase 10E2 native release-projection backend sub-slice implemented: module catalog/governance lifecycle now projects `release_status`, `release_state`, `rollback_target` and `data_access` summaries for current release and post-rollback state; release-record sync carries the same data-access projection; runtime-installed release/rollback resolves the actual installed manifest root before writing source snapshots. Historical checkpoint: at this point `P10-BE-RELEASE-PROJECTION` still needed Shell/App Store/Settings reload evidence and Browser/Rust release smoke; the later Phase 10F/10H row closes the App Store Browser/Rust part and the current blocker ledger marks this row closed. Projection-failure and duplicate-command repair behavior were closed later by 10D2. | `cargo test --bin ctox module_catalog_projects_release_state_data_access_and_rollback_target -- --nocapture` - 1 passed; `cargo test --bin ctox module_release_ -- --nocapture` - 11 passed, 0 failed; `cargo test --bin ctox module_ -- --nocapture` - 40 passed, 0 failed; `rustfmt --edition 2021 src/core/business_os/store.rs --check`; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `cargo check --bin ctox`; `cargo test --manifest-path src/core/rxdb/Cargo.toml` - 239 unit tests and 30 conformance tests passed; `node src/apps/business-os/rxdb/tests/run-all.mjs` - 37 passed, 0 failed, 2 skipped (`cross-process-file-fetch-smoke`, `cross-process-wire-smoke`) |
| 2026-06-17 | Production-ready plan expanded to cover the remaining full-product rings instead of only release/audience/UI work: dynamic-app runtime safety, browser storage authority, storage-boundary browser proof, performance/scale budgets, backup/restore drills, support artifact schema, security/privacy signoff and customer/operator docs are now first-class blocker-ledger rows, implementation slices, completion checklist entries, coverage-map rows and open decisions. No implementation status was advanced by this docs-only update. | Source validation against `src/apps/business-os/app.js` dynamic import/module context/storage paths, `src/apps/business-os/modules/app-store/index.js` storage/fetch paths, `src/apps/business-os/modules/creator/index.js` storage/fetch paths, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`; docs-only update |
| 2026-06-17 | Phase 10E3 UI/static partial implemented: shared app lifecycle logic now derives release, rollback and data-access summaries from native `lifecycle.release_state`, `lifecycle.rollback_target` and `lifecycle.data_access`; App Store cards/details and Shell lifecycle drawer render projected Freigabe/Rollback/Datenzugriff facts with business-facing data-area labels. At this checkpoint `P10-BE-RELEASE-PROJECTION` still needed Settings consumption plus Browser/Rust reload evidence; the Settings static-consumption gap is closed by the next update-log row. | `node src/apps/business-os/shared/app-lifecycle.test.mjs` - 11 passed; `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 17 passed; `node --check src/apps/business-os/shared/app-lifecycle.js && node --check src/apps/business-os/modules/app-store/index.js && node --check src/apps/business-os/app.js`; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `git diff --check --` touched JS/CSS/docs paths; trailing-whitespace grep over touched JS/CSS/docs paths |
| 2026-06-17 | Phase 10E3 Settings UI/static consumption added: Settings module-management rows now use the same shared release projection as Shell/App Store and render Freigabe, Rollback, Datenzugriff and evidence-only Review facts from native lifecycle state. Historical checkpoint: at this point `P10-BE-RELEASE-PROJECTION` still needed Browser/Rust reload evidence that success waits for projected state, and `P10-SETTINGS-FALLBACK` still needed read-only downgrade evidence; both are superseded by later rows. | `node src/apps/business-os/shared/react-settings.test.mjs` - 5 passed; `node src/apps/business-os/shared/app-lifecycle.test.mjs` - 11 passed; `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 17 passed; `node --check src/apps/business-os/shared/react-settings.js`; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `git diff --check --` touched JS/CSS/docs paths; trailing-whitespace grep over touched JS/CSS/docs paths |
| 2026-06-17 | Phase 10G Settings fallback static downgrade added: Settings no longer exposes active `ctox.module.release` or `ctox.module.rollback` dispatch controls; release and rollback in Settings are read-only diagnostics with disabled controls, while the future active publish path remains the App Store flow. `P10-SETTINGS-FALLBACK` remains open only for Browser/Rust proof that the real Settings drawer renders the disabled/read-only state. | `node src/apps/business-os/shared/react-settings.test.mjs` - 6 passed; `node --check src/apps/business-os/shared/react-settings.js`; `rg -n "ctox\\.module\\.release|ctox\\.module\\.rollback|data-module-release=|data-module-rollback=|data-rollback-version|releaseModule\\(|rollbackModule\\(" src/apps/business-os/shared/react-settings.js src/apps/business-os/shared/react-settings.test.mjs`; static source guard in `react-settings.test.mjs`; docs updated |
| 2026-06-17 | Phase 10G Settings fallback Browser/Rust proof completed: the Shell smoke API can open the real Settings drawer, the smoke seeds release history, and the live drawer proves Freigabe/Rollback are disabled diagnostics with no active Settings release/rollback data controls. `P10-SETTINGS-FALLBACK` is closed. Historical status note: Phase 10 still needed App Store wizard, release smoke, Settings Activity/browser audit evidence and the later 10D2 projection/repair closure. | `node --check src/apps/business-os/app.js src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/shared/react-settings.test.mjs` - 6 passed; `node src/apps/business-os/shared/app-lifecycle.test.mjs` - 11 passed; `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 17 passed; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-roles-permissions-ui SMOKE_MATRIX_ATTEMPTS=2 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=240000 BUSINESS_PORT=18988 SIGNALING_PORT=28988 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK, `business_os_roles_permissions_settings_release_fallback_readonly=1`, reload count 0, hook wait 79ms, browser errors/404/request failures 0 |
| 2026-06-17 | Phase 10D2 backend consistency completed: replaying an accepted release command is idempotent and does not duplicate release rows, manual source-version snapshots or audit events; `ctox.module.repair_lifecycle_projection` repairs canonical release projections, RxDB release rows and the module catalog through the existing RxDB/WebRTC command path. Historical checkpoint: `P10-BE-CONSISTENCY` closed here, while App Store wizard, release reload/success-wait browser evidence, Settings/browser audit labels and Browser/Rust release smoke were still open until the later Phase 10F/10H and `P10-BE-AUDIT` rows. | `rustfmt --edition 2021 src/core/business_os/store.rs src/core/business_os/rxdb_peer.rs --check`; `cargo test --bin ctox module_release_command_replay_does_not_duplicate_release_state --target-dir runtime/build/core-rxdb-integration-target -- --nocapture` - 1 passed; `cargo test --bin ctox module_lifecycle_projection_repair_resyncs_releases_and_catalog --target-dir runtime/build/core-rxdb-integration-target -- --nocapture` - 1 passed; `cargo test --bin ctox module_release_ --target-dir runtime/build/core-rxdb-integration-target -- --nocapture` - 12 passed; `cargo test --bin ctox module_ --target-dir runtime/build/core-rxdb-integration-target -- --nocapture` - 42 passed; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `cargo check --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` |
| 2026-06-17 | Production-ready plan revalidated after backend/data-plane and UI/smoke read-only subagent passes: stale Phase-10 historical status wording was clarified, Phase 11/12/14 blocker anchors were tightened to concrete source functions, and launcher/start-menu lifecycle/version badge coverage was added as a first-class production gap with JS and Browser/Rust evidence requirements. The dynamic-app concept source-anchor table now uses function-level anchors and explicitly calls out `buildStartMenuItem` as the missing launcher badge surface. | Read-only subagents Cicero and Pasteur; source validation against `src/core/business_os/store.rs::projected_module_lifecycle`, `src/core/business_os/policy.rs::evaluate`, `src/core/business_os/mcp_channel.rs::business_os_mcp_policy_decision`, current MCP visibility helpers in `src/core/business_os/mcp_channel.rs`, `src/apps/business-os/app.js::buildStartMenuItem`, `src/apps/business-os/app.js::renderModuleTab`, `src/apps/business-os/app.js::renderModuleAppBar`, `src/apps/business-os/app.js::openAppLifecycleDrawer`, `src/apps/business-os/modules/app-store/index.js::rawCatalogItems`, `src/apps/business-os/modules/app-store/index.js::releaseProjectionBadgeHtml`, `src/core/rxdb/tools/browser_rust_smoke.js`, `src/core/rxdb/tools/browser_rust_smoke_matrix.js`; docs-only update |
| 2026-06-17 | Phase 13B real Shell guarded-facade partial implemented and the production plan updated: normal dynamic module contexts now call `createLiveDbFacade(mod)`, and the Browser/Rust dynamic-app smoke matrix requires real-context collection, property, cached-handle and raw denial plus cached-handle read success after an explicit `data.read` grant. Historical status note: at this checkpoint Phase 13 still needed inventory, raw/cached static lint, persisted `openModule`/reload or fresh-profile fixture, packaged/core migration or exceptions, dynamic runtime-safety boundary and browser-storage scoping; the later 13A/13D rows close the inventory and static-lint slices, the later Phase 13B closeout row closes persisted `openModule`/reload proof, and the 2026-06-18 Phase 13E/13F rows close dynamic runtime-safety and browser-storage scope. | `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=240000 BUSINESS_PORT=18989 SIGNALING_PORT=28989 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_real_context_collection_denied=1`, `business_os_dynamic_real_context_property_denied=1`, `business_os_dynamic_real_context_cached_denied=1`, `business_os_dynamic_real_context_raw_denied=1`, `business_os_dynamic_real_context_cached_read_grant_allowed=1`, browser errors/404/request failures 0 and one non-blocking IndexedDB-closing warning during shutdown |
| 2026-06-17 | Phase 13A DB-isolation inventory completed: every current packaged/core module, desktop app and unscoped Shell DB facade is classified with owner, review date, migration/exception status and current DB-access shape in `docs/business-os-db-isolation-inventory.json`. The guard fails when a module, desktop app or unscoped facade drifts without an inventory update. `P13-MODULE-INVENTORY` is closed; historical status note: at this checkpoint Phase 13 still needed migration/exception implementation, persisted Shell-context proof, runtime-safety and storage-scope work. | `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node --check src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`; `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null` |
| 2026-06-17 | Phase 13D static DB-access drift guard completed: `assert-db-isolation-inventory.mjs` now detects raw DB access, collection-property access, `ctx.db.collections` proxy access and cached/exported DB handles, including optional raw access and helper-forwarded raw handles. `assert-module-conformance.mjs` invokes that guard, so the standard module gate fails when a module adds one of those bypass shapes without an explicit inventory update. Every packaged/core module now has explicit Boolean `db_access` flags, including false values. `P13-RAW-DB-LINT` is closed for static drift coverage; Phase 13C remains responsible for migrating or narrowly approving the inventoried legacy access. | `node src/apps/business-os/scripts/assert-module-conformance.mjs`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 7 cases; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node --check src/apps/business-os/scripts/assert-module-conformance.mjs`; `node --check src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` |
| 2026-06-17 | Phase 13B real Shell guarded-facade closeout completed: the Browser/Rust dynamic-app smoke now creates a runtime-installed fixture in the installed-app state root, persists it through `business_module_catalog`, reloads the Shell and opens it through real `openModule(mod)`. The mounted app proves collection, property, cached-handle and raw DB access are denied without data grants, while the existing real-context helper path still proves cached read succeeds after explicit `data.read`. `P13-REAL-SHELL-DB-PATH` is closed. | `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with pre-existing warnings; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=240000 BUSINESS_PORT=18989 SIGNALING_PORT=28989 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with all `business_os_dynamic_open_module_*` keys 1 and browser errors/404/request failures 0 |
| 2026-06-17 | Phase 14A smoke-mode registry completed: required Business OS production modes for release, audience, agent scope, auth scope and fresh profile are registered in the runner and matrix with fixed evidence contracts. The matrix self-test fails on missing runner mode, missing matrix mode, missing evidence requirement, missing required evidence key and unsupported evidence mode. Historical checkpoint: at this point all five modes were registered fail-fast placeholders; the later Phase 10F/10H row supersedes that for `business-os-app-release-ui`. `P14-SMOKE-MODES-REGISTERED` is closed; Phase 14 remains open for actual auth/tenant/fresh-profile/storage/scale browser flows. | `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MODES=__missing_business_os_smoke__ node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - exited with expected unsupported-mode configuration error; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODE=business-os-app-release-ui SMOKE_PAGE_PATH=/index.html node src/core/rxdb/tools/browser_rust_smoke.js` - exited with expected placeholder message |
| 2026-06-17 | Phase 16 static required-smoke-mode CI guard completed: the existing x86_64 Linux Business OS RxDB-only contract job now syntax-checks the production smoke registry and runs the matrix self-test, so CI fails when required Business OS production smoke modes or evidence contracts drift. `P16-SMOKE-REQUIRED-MODES-GUARD` is closed; the broader production gate, smoke artifact schema and release workflow prerequisites remain open. | `.github/workflows/ci.yml` updated; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `git diff --check -- .github/workflows/ci.yml src/core/rxdb/tools/business_os_production_smoke_registry.js src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js docs/business-os-roles-permissions-plan.md docs/business-os-dynamic-apps-permissions-concept.md` |
| 2026-06-17 | Phase 10F App Store release UI/payload partial completed: runtime-installed apps now expose a permission-gated `Freigeben` action, and the release dialog submits the existing `ctox.module.release` command with target version, source snapshot, rollback target, responsible users, release notes and evidence-only data review. Historical checkpoint: `P10-UI-WIZARD` still needed the real Browser/Rust publish/reload/rollback proof; the next row closes it. | `node --check src/apps/business-os/modules/app-store/index.js`; `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 18 passed; `git diff --check -- src/apps/business-os/modules/app-store/index.js src/apps/business-os/modules/app-store/index.css src/apps/business-os/modules/app-store/app-store.test.mjs` |
| 2026-06-17 | Phase 10F/10H App Store release Browser/Rust proof completed: `business-os-app-release-ui` now seeds a runtime-installed private `0.8.0` app, proves only the App-Verantwortliche can see it before release, publishes `1.0.0` through the real App Store dialog and native `business_commands`, waits for native catalog projection, verifies Team visibility, projected `v1.0.0`/`Team` badge, data-review details, clean reload, localStorage non-authority and rollback via the version dialog. Historical checkpoint: `P10-UI-WIZARD` and `P10-BE-RELEASE-PROJECTION` were closed here, while `P10-BE-AUDIT` was closed by the later Activity row. | `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 18 passed; `node src/apps/business-os/shared/app-lifecycle.test.mjs` - 11 passed; `node src/apps/business-os/shared/react-settings.test.mjs` - 6 passed; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `node src/apps/business-os/scripts/assert-module-conformance.mjs`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `cargo fmt --check`; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo build --bin ctox`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-release-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60075 SIGNALING_PORT=60076 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with all required release evidence keys true, `browser_warning_count=0`, `browser_error_count=0`, `browser_resource_404_count=0`, `browser_request_failure_count=0` |
| 2026-06-17 | Phase 10E1/P10-BE-AUDIT Activity browser evidence completed: `ctox.module.rollback_version` now records the same business rollback lifecycle audit type as release rollback, Settings Activity renders release/rollback lifecycle events with business-facing labels and redacts raw event names, command types and data-review internals, and `business-os-app-release-ui` proves the real Settings drawer shows both audit rows after publish/rollback. Historical checkpoint: Phase 10 was complete locally here; at that point Phases 11-16 were still the full-product production-readiness work. Current status is tracked in the Phase Tracker. | `node --check src/apps/business-os/shared/react-settings.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `rustfmt --edition 2021 src/core/business_os/store.rs --check`; `cargo fmt --check`; `node src/apps/business-os/shared/react-settings.test.mjs` - 6 passed; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_source_rollback_version_uses_apps_rollback_without_modify_permission -- --nocapture` - 1 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_rollback_commands_persist_failed_outcomes -- --nocapture` - 1 passed; `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 18 passed; `node src/apps/business-os/shared/app-lifecycle.test.mjs` - 11 passed; `node src/apps/business-os/scripts/assert-rxdb-only.mjs`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test`; `node src/apps/business-os/scripts/assert-module-conformance.mjs`; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo build --bin ctox`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-release-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60075 SIGNALING_PORT=60076 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_app_release_release_audit_visible=1`, `business_os_app_release_rollback_audit_visible=1`, `business_os_app_release_activity_audit_redacted=1`, `browser_warning_count=0`, `browser_error_count=0`, `browser_resource_404_count=0`, `browser_request_failure_count=0` |
| 2026-06-17 | Phase 11A visibility-grant split completed for the dedicated grant path: `apps.view` now exists in native/browser permission models, non-public Browser lifecycle visibility uses `apps.view`, native preview projection reads only active `apps.view` grants, and exact `apps.modify` no longer makes private/restricted apps visible. The dynamic-app Browser/Rust smoke was hardened against background catalog-refresh races. `P11-AUDIENCE-GRANT` is closed; `P11-AUDIENCE-PERSISTENCE`, responsibility safety, audience UI and deep-link/fresh-profile proof remain open. | `node --check src/apps/business-os/shared/permissions.js`; `node --check src/apps/business-os/shared/app-lifecycle.js`; `node --check src/apps/business-os/shared/app-lifecycle.test.mjs`; `node --check src/apps/business-os/shared/permissions.test.mjs`; `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node src/apps/business-os/shared/app-lifecycle.test.mjs` - 11 passed; `node src/apps/business-os/shared/permissions.test.mjs` - 7 passed; `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 18 passed; `node src/apps/business-os/shared/shell-permissions-ui.test.mjs` - 4 passed; `rustfmt --edition 2021 src/core/business_os/policy.rs src/core/business_os/store.rs --check`; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox business_os::policy -- --nocapture` - 3 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_governance_projection_includes_permission_model_and_grants -- --nocapture` - 1 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_catalog_projects_runtime_app_lifecycle_backfill -- --nocapture` - 1 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo build --bin ctox`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=2 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60145 SIGNALING_PORT=60146 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with private/team/restricted visibility keys, data-denial/grant keys, reload evidence, browser warnings/errors/404/request failures 0 |
| 2026-06-17 | Phase 11B/11D audience persistence and route-lock slice completed: legacy runtime-app `lifecycle.preview_user_ids` are idempotently backfilled into native module-scoped `apps.view` grants, projected preview users now derive from active grants, `openModule` denies hidden private/preview/restricted apps before import/mount and redirects to a visible fallback, and the registered `business-os-app-audience-ui` Browser/Rust smoke is implemented and passing. `P11-AUDIENCE-PERSISTENCE` and `P11-DEEPLINK-LOCKED-STATE` are closed; `P11-RESPONSIBILITY-ORPHAN` and `P11-BADGE-DRAWER-PERMISSIONS` remain open. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/shared/app-lifecycle.test.mjs`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `node src/apps/business-os/shared/app-lifecycle.test.mjs` - 12 passed; `rustfmt --edition 2021 src/core/business_os/store.rs --check`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_catalog_projects_runtime_app_lifecycle_backfill -- --nocapture` - 1 passed; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-audience-ui SMOKE_MATRIX_ATTEMPTS=2 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60161 SIGNALING_PORT=60162 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with private hidden, preview target visible, preview/restricted outside hidden, deep-link locked, reload, fresh-profile, storage-boundary keys 1 and browser warnings/errors/404/request failures 0 |
| 2026-06-17 | Phase 11C responsibility safety completed: private runtime apps cannot lose their last active App-Verantwortliche:r through `ctox.module.assign_founder` or `ctox.business_os.user.upsert` unless the acting Owner/Admin explicitly accepts recovery responsibility. Recovery writes `business_module_acl` and `business_events`, failed command attempts are persisted as failed `business_commands`, and inactive users no longer project as active App-Verantwortliche or Founder permission assignments. `P11-RESPONSIBILITY-ORPHAN` is closed; `P11-BADGE-DRAWER-PERMISSIONS` remains the only open Phase-11 blocker. | `rustfmt --edition 2021 src/core/business_os/store.rs src/core/business_os/rxdb_peer.rs --check`; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_founder_assignment -- --nocapture` - 3 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox user_deactivation_requires_recovery_for_sole_private_app_responsibility -- --nocapture` - 1 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_assign_founder_command_persists_failed_orphan_outcome -- --nocapture` - 1 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox user_upsert_command_persists_failed_private_app_responsibility_outcome -- --nocapture` - 1 passed; `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/core-rxdb-integration-target cargo test --bin ctox module_catalog_projects_runtime_app_lifecycle_backfill -- --nocapture` - 1 passed |
| 2026-06-17 | Phase 11D badge/drawer/launcher UX completed: launcher/start-menu app-choice items render runtime app version plus lifecycle badges, badge clicks open the lifecycle drawer without launching the app, and the drawer shows manager vs Team-read-only states with business-facing copy. `P11-BADGE-DRAWER-PERMISSIONS` is closed and Phase 11 is complete locally. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/shared/shell-permissions-ui.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs` - 5 passed; `node --test src/apps/business-os/shared/app-lifecycle.test.mjs` - 13 passed; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODE=business-os-dynamic-apps-ui SMOKE_PAGE_PATH=/index.html /Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node src/core/rxdb/tools/browser_rust_smoke.js` - pass with `business_os_dynamic_launcher_badges_visible=1`, `business_os_dynamic_lifecycle_drawer_manager_state=1`, `business_os_dynamic_lifecycle_drawer_readonly_state=1`, browser warning/error/404/request failure counts 0 |
| 2026-06-17 | Phase 12A MCP app visibility/data split completed: MCP module listing now exposes apps through lifecycle public state or exact `apps.view` rather than data grants; module detail/entities/actions/proposals require app visibility before `data.read`; module links require app visibility only; execution requires app visibility before `data.write`. `P12-MCP-APP-VISIBILITY` is closed. Historical checkpoint: remaining Phase 12 work was browser AI scope panels, submitted client-context integrity, agent grant-management UX and Browser/Rust agent-scope smoke; the global Agent-Scope Browser/Rust smoke is closed by the 2026-06-18 row below. | `rustfmt --edition 2021 --check src/core/business_os/mcp_channel.rs`; `cargo test --bin ctox mcp_business_os_policy -- --nocapture` - 18 passed, 0 failed. The first targeted run found and fixed a test-fixture path issue (`root/business-os` vs real `root/runtime/business-os` installed-app root), then `cargo test --bin ctox mcp_business_os_policy_filters_module_list_by_app_visibility_not_data_read -- --nocapture` passed before the full policy group was rerun. |
| 2026-06-17 | Phase 12B/12C first browser slice completed for global right-click and command context: the real Shell CTOX context menu renders actor/app/selection/data/external scope from `buildGlobalCtoxAgentScopeView`, submits the same object as `client_context.visible_scope`, the command bus canonicalizes module/app/action/mode/target/record/scope aliases while preserving caller actors, and Coding Agents add external provider/workspace/session scope to their submitted context. Historical checkpoint: App Store, Business Chat visible panels, Browser/Rust smoke and audit parity were still open here; later 2026-06-18 rows close them. | Read-only subagent Galileo confirmed the current right-click/Business Chat/Coding Agents/command-bus gaps; implementation evidence: `node --check src/apps/business-os/shared/shell-permissions-ui.js`; `node --check src/apps/business-os/shared/command-bus.js`; `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/coding-agents/index.js`; `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs` - 7 passed; `node --test src/apps/business-os/shared/command-bus.test.mjs` - 2 passed; `git diff --check -- src/apps/business-os/shared/shell-permissions-ui.js src/apps/business-os/shared/shell-permissions-ui.test.mjs src/apps/business-os/shared/command-bus.js src/apps/business-os/shared/command-bus.test.mjs src/apps/business-os/app.js src/apps/business-os/app.css src/apps/business-os/modules/coding-agents/index.js`. |
| 2026-06-17 | Phase 12C scheduled Business Chat context preservation added: scheduled chat commands now merge existing `chat.contextMeta.client_context` before adding fresh chat/message/attachment metadata, so a right-click-created chat does not lose its visible scope when scheduled execution later dispatches the command. Historical checkpoint: browser comparison and audit parity were still open here; later 2026-06-18 rows close both. | `node --check src/apps/business-os/shared/business-chat.js`; `git diff --check -- src/apps/business-os/shared/business-chat.js`; plan/concept updated at that checkpoint. |
| 2026-06-18 | Phase 12B/12C Business Chat visible-scope panel added: chats opened from scoped context now render the preserved `client_context.visible_scope`/`scope.visible_scope` as the same business-facing `CTOX Zugriff` panel used by the Shell context menu, while chats without visible scope stay visually unchanged. Historical checkpoint: App Store panel, Browser/Rust visible-vs-submitted comparison and native/MCP audit parity were still open here; later 2026-06-18 rows close them. | `node --check src/apps/business-os/shared/business-chat.js`; `node --test src/apps/business-os/shared/business-chat.test.mjs` - 3 passed; `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs` - 7 passed; `node --test src/apps/business-os/shared/command-bus.test.mjs` - 2 passed; `git diff --check -- src/apps/business-os/shared/shell-permissions-ui.js src/apps/business-os/shared/shell-permissions-ui.test.mjs src/apps/business-os/shared/command-bus.js src/apps/business-os/shared/command-bus.test.mjs src/apps/business-os/shared/business-chat.js src/apps/business-os/shared/business-chat.test.mjs src/apps/business-os/app.js src/apps/business-os/app.css src/apps/business-os/modules/coding-agents/index.js docs/business-os-roles-permissions-plan.md docs/business-os-dynamic-apps-permissions-concept.md`. |
| 2026-06-18 | Phase 12B/12C App Store context-chat scope panel added: the App Store right-click menu now renders the shared `CTOX Zugriff` panel from selected app lifecycle/data projection, submits selected app `module_id`/`app_id`, actor and `visible_scope` in `client_context`, and downgrades direct disallowed app-modify detail construction to data mode. Historical checkpoint: Browser/Rust visible-vs-submitted comparison and native/MCP audit parity were still open here; later 2026-06-18 rows close them. | `node --check src/apps/business-os/modules/app-store/index.js`; `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 18 passed; `node --test src/apps/business-os/shared/business-chat.test.mjs` - 3 passed; `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs` - 7 passed; `node --test src/apps/business-os/shared/command-bus.test.mjs` - 2 passed; `git diff --check -- src/apps/business-os/modules/app-store/index.js src/apps/business-os/modules/app-store/app-store.test.mjs src/apps/business-os/shared/shell-permissions-ui.js src/apps/business-os/shared/shell-permissions-ui.test.mjs src/apps/business-os/shared/command-bus.js src/apps/business-os/shared/command-bus.test.mjs src/apps/business-os/shared/business-chat.js src/apps/business-os/shared/business-chat.test.mjs src/apps/business-os/app.js src/apps/business-os/app.css src/apps/business-os/modules/coding-agents/index.js docs/business-os-roles-permissions-plan.md docs/business-os-dynamic-apps-permissions-concept.md`. |
| 2026-06-18 | Phase 12B/12C/12D Agent-Scope Browser/Rust proof expanded: `business-os-agent-scope-ui` seeds a full-workspace runtime app, opens it through the real Shell, submits via the real CTOX right-click context menu and the real App Store context menu, proves both visible `CTOX Zugriff` panels match submitted `client_context.visible_scope`, proves Business Chat renders the same submitted scope, proves Settings Admin renders active Sonderfreigaben as a read-only Owner/Admin boundary, proves hidden private app open is denied, proves data read is denied before and allowed after an exact grant, proves write stays denied without `data.write`, and proves persisted `business_commands.client_context.visible_scope` remains audit-visible. Historical checkpoint: at this point native/MCP audit parity was still open; the 2026-06-18 audit-metadata row closes it. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/app-store/index.js`; `node --check src/apps/business-os/shared/react-settings.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/modules/app-store/app-store.test.mjs` - 18 passed; `node src/apps/business-os/shared/react-settings.test.mjs` - 7 passed; `node --test src/apps/business-os/shared/business-chat.test.mjs` - 3 passed; `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs` - 7 passed; `node --test src/apps/business-os/shared/command-bus.test.mjs` - 2 passed; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; first Browser/Rust run passed feature keys but failed startup budget (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=60135`); final rerun `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-agent-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60241 SIGNALING_PORT=60242 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_agent_scope_panel_visible=1`, `business_os_agent_scope_client_context_matches_ui=1`, `business_os_agent_scope_app_store_panel_visible=1`, `business_os_agent_scope_app_store_context_matches_ui=1`, `business_os_agent_scope_business_chat_scope_matches_context=1`, `business_os_agent_scope_settings_grant_boundary_visible=1`, all existing data-denial/grant/audit keys true and browser warnings/errors/404/request failures 0. |
| 2026-06-18 | Phase 12 native/MCP audit metadata parity completed: native policy audit events now store redacted, scope-only `client_context` with app/module/record and `visible_scope`; MCP activity metadata now stores `business_scope` for tool, module, action, collection, record and command identifiers while excluding free-form prompt, selected text, title, objective, query and payload content. `P12-AUDIT-METADATA`, `P12-AGENT-SCOPE` and `P12-CLIENT-CONTEXT-INTEGRITY` are closed; Phase 12 is locally complete. Full-product production readiness still waits on Phases 13-16. | `rustfmt --edition 2021 src/core/business_os/store.rs src/core/business_os/mcp_channel.rs --check`; `cargo test --bin ctox allowed_policy_decision_writes_business_event_audit -- --nocapture` - 1 passed; `cargo test --bin ctox audited_mcp_policy_denial_records_business_os_policy_decision -- --nocapture` - 1 passed; `cargo test --bin ctox audited_mcp_read_denial_records_business_os_policy_decision -- --nocapture` - 1 passed; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`; first post-build Browser/Rust rerun passed all feature keys but failed startup budget (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=60153`); final rerun `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-agent-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60351 SIGNALING_PORT=60352 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with all agent-scope evidence keys true, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0`, `startup_smoke_hook_wait_ms=43`. |
| 2026-06-18 | Phase 13E dynamic-app runtime safety completed: runtime-installed apps keep the explicit same-origin trusted-code boundary, `createModuleContext` exposes `ctx.runtimeCapabilities` with the `business-os-runtime-capabilities-v1` contract, the installed App Creator validator rejects forbidden generated-app network/import/storage/Shell-global/cached-DB/Worker/navigation/evaluator/control-command bypasses, and `business-os-dynamic-apps-ui` proves the real persisted runtime app sees the runtime-safety contract after reload through `openModule`. `P13-DYNAMIC-RUNTIME-SAFETY` is closed; Phase 13 still remains open for packaged/core migration or tested exceptions. | `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node --check src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs`; `node --check src/apps/business-os/scripts/validate-app-module.mjs`; `node --check src/apps/business-os/scripts/validate-app-module.test.mjs`; `node src/apps/business-os/scripts/validate-app-module.test.mjs` - OK; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 11 cases after the 13D hardening rerun; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60431 SIGNALING_PORT=60432 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_runtime_safety_contract=1`, `business_os_dynamic_runtime_safety_capabilities=1`, existing dynamic app data-guard keys true, browser warnings/errors/404/request failures 0 and `startup_smoke_hook_reload_count=0`. |
| 2026-06-18 | Phase 13F browser storage scope completed: Shell UI preference storage now uses scoped keys for taskbar pins, module layout, account preferences, Shell column/module resizer widths and Pairing config; modules receive `ctx.storageScope` with the `business-os-storage-scope-v1` contract, and App Store pane width uses that facade. Dynamic Apps smoke proves scoped keys and module storage contract, Audience smoke tampers both legacy and active scoped taskbar storage without widening private/preview/restricted visibility, and Release smoke keeps release/rollback storage-boundary evidence green. `P13-BROWSER-STORAGE-SCOPE` is closed; Phase 13 still remains open for packaged/core migration or tested exceptions. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/app-store/index.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 11 cases after the 13D hardening rerun; `node src/apps/business-os/scripts/validate-app-module.test.mjs` - OK; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; first Dynamic Apps Browser/Rust run passed feature keys but failed startup budget (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=60780`); final dynamic rerun `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60463 SIGNALING_PORT=60464 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_storage_keys_scoped=1`, `business_os_dynamic_storage_scope_contract=1`, browser warnings/errors/404/request failures 0 and `startup_smoke_hook_reload_count=0`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-audience-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60471 SIGNALING_PORT=60472 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_app_audience_storage_boundary_checked=1`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-app-release-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60481 SIGNALING_PORT=60482 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_app_release_storage_boundary_checked=1`. |
| 2026-06-18 | Phase 13D DB-access inventory guard hardened after source read for the next P13C batch: optional chaining, dynamic collection property access and local DB aliases are now detected. The inventory was updated to match current source truth for `coding-agents`, `creator`, `ctox`, `customers`, `cv-print-builder`, `iot`, `notes` and `support`; this does not close `P13-CORE-DB-ISOLATION`, but prevents the migration plan from relying on false "no DB access" entries. | `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 11 cases; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `node --check src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`; `node --check src/apps/business-os/scripts/assert-module-conformance.mjs`. |
| 2026-06-18 | Phase 13C first packaged user-module migration slice completed for `conversations`: `createLiveDbFacade(mod)` now activates the guarded DB facade for runtime-installed modules plus `conversations`, `ctx.runtimeCapabilities.database` reports guarded raw/property/cached handles for that module, and the inventory marks only `conversations` as `guarded-facade-migrated`. Two early smoke assumptions were corrected during validation: `conversations` is not guaranteed to be present in `state.modules` during that boot phase, and `communication_messages` is not registered in this smoke data plane; the final proof uses the manifest-declared `business_commands` collection. `P13-CORE-DB-ISOLATION` remains in progress for the remaining packaged/core modules and system/internal exceptions. | `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`; final Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60531 SIGNALING_PORT=60532 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_module=conversations`, `business_os_dynamic_packaged_guard_collection=business_commands`, all packaged guard denial/grant keys 1, browser warnings/errors/404/request failures 0 and `startup_smoke_hook_reload_count=0`. |
| 2026-06-18 | Phase 13C packaged/starter user-module batch expanded and verified for `conversations`, `cv-print-builder`, `documents` and `spreadsheets`: `createLiveDbFacade(mod)` now activates the guarded DB facade for runtime-installed modules plus this batch, `ctx.runtimeCapabilities.database` reports guarded raw/property/cached handles for all four modules, and the inventory marks only this batch as `guarded-facade-migrated`. `research`, `support` and `shiftflow` were source-read and deliberately left open because they still carry broader raw/local alias or cached-handle risk. `P13-CORE-DB-ISOLATION` remains in progress for the remaining packaged/core modules and system/internal exceptions. | `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 11 cases; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings. First Browser/Rust attempt could not start before the binary existed; the next run passed feature keys but failed startup budget (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=60123`); the following matrix-green run still had one transient IndexedDB-closing desktop icon render browser error, so it was not accepted as clean evidence. Final Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60651 SIGNALING_PORT=60652 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_modules=conversations,cv-print-builder,documents,spreadsheets`, `business_os_dynamic_packaged_guard_count=4`, `business_os_dynamic_packaged_guard_batch_coverage=1`, all `business_os_dynamic_packaged_guard_all_*` denial/grant keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=37`. |
| 2026-06-18 | Phase 13C Support guarded-module slice completed after read-only source validation: `support` is added to `GUARDED_PACKAGED_DATA_MODULE_IDS`, `createModuleContext(mod)` now exposes `ctx.permissions` next to `ctx.db`, the Shell renders a business-facing locked state when Support is visible but data grants are missing, and expected permission locks no longer count as browser console errors. `customers`, `research` and `shiftflow` remain open by source evidence: optional cross-module reads, registry/manifest drift plus document/knowledge reads, and cached global/DOM DB handles respectively. `P13-CORE-DB-ISOLATION` stays in progress for the remaining module migrations and system/internal exceptions. | `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 11 cases; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; final Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60781 SIGNALING_PORT=60782 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_modules=conversations,cv-print-builder,documents,spreadsheets,support`, `business_os_dynamic_packaged_guard_count=5`, `business_os_dynamic_packaged_guard_shell_locked_state=1`, `business_os_dynamic_packaged_guard_context_permission_facade=1`, `business_os_dynamic_packaged_guard_all_context_permission_facades=1`, all packaged guard deny/grant/write keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=82`. |
| 2026-06-18 | Phase 13C Customers/Coding Agents guarded-module slice completed after read-only source validation: `customers` and `coding-agents` are added to `GUARDED_PACKAGED_DATA_MODULE_IDS`, the Dynamic Apps smoke now registers missing module-owned schemas per packaged guard spec, and the inventory marks exactly `coding-agents`, `conversations`, `customers`, `cv-print-builder`, `documents`, `spreadsheets` and `support` as guarded-facade migrated at this checkpoint. `customers` degrades optional linked cross-app projections to an empty linked-data state on permission denial instead of aborting the core CRM load. `research` was deliberately kept out of the guarded batch; its manifest/fallback now include `business_commands` and `knowledge_tables`, but document collection reads for Fachbericht content still need explicit grants or UI feature gates. `invoices` was the next source-validated candidate and is closed by the immediately following smoke slice. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/customers/index.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `git diff --check -- src/apps/business-os/app.js src/apps/business-os/modules/customers/index.js src/apps/business-os/modules/customers/locales/de.json src/apps/business-os/modules/customers/locales/en.json src/apps/business-os/modules/research/module.json src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js docs/business-os-db-isolation-inventory.json`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; accepted Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61071 SIGNALING_PORT=61072 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_module=coding-agents`, `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`, `business_os_dynamic_packaged_guard_modules=coding-agents,conversations,customers,cv-print-builder,documents,spreadsheets,support`, `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,business_commands,customer_accounts,business_commands,business_commands,business_commands,business_commands`, `business_os_dynamic_packaged_guard_count=7`, `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard deny/grant/write keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=74`. `node --test src/apps/business-os/modules/customers/customers.test.mjs` was attempted separately but could not run in this workspace because `esbuild` is not installed/resolvable for that module test. |
| 2026-06-18 | Phase 13C Invoices guarded-module slice completed after source validation: `invoices` is added to `GUARDED_PACKAGED_DATA_MODULE_IDS`, the packaged guard spec registers `/modules/invoices/schema.js` and smokes the module-owned `accounting_invoices` collection, and the inventory now marks exactly `coding-agents`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `spreadsheets` and `support` as guarded-facade migrated. The old `window.__ctoxInvoicesModule = { mount, STATE }` bridge no longer exposes module state, `ctx` or `ctx.db`; it now exposes only `mount` and a redacted `inspect()` snapshot. `P13-CORE-DB-ISOLATION` remains open for the still source-validated cleanup candidates `research`, `shiftflow`, `iot`, `outbound`, `notes`, `calendar`, `buchhaltung` and `matching`. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/customers/index.js`; `node --check src/apps/business-os/modules/invoices/index.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`; `python3 -m json.tool src/apps/business-os/modules/research/module.json >/dev/null`; `python3 -m json.tool src/apps/business-os/modules/customers/locales/de.json >/dev/null`; `python3 -m json.tool src/apps/business-os/modules/customers/locales/en.json >/dev/null`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/modules/invoices/tests/invoice-types.test.mjs` - 9/9 passed; `git diff --check -- src/apps/business-os/app.js src/apps/business-os/modules/customers/index.js src/apps/business-os/modules/customers/locales/de.json src/apps/business-os/modules/customers/locales/en.json src/apps/business-os/modules/invoices/index.js src/apps/business-os/modules/research/module.json src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js docs/business-os-db-isolation-inventory.json docs/business-os-roles-permissions-plan.md docs/business-os-dynamic-apps-permissions-concept.md`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; accepted Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61081 SIGNALING_PORT=61082 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_module=coding-agents`, `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`, `business_os_dynamic_packaged_guard_modules=coding-agents,conversations,customers,cv-print-builder,documents,invoices,spreadsheets,support`, `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,business_commands,business_commands`, `business_os_dynamic_packaged_guard_count=8`, `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard deny/grant/write keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=68`. `node --test src/apps/business-os/modules/customers/customers.test.mjs` remains unproven in this workspace because `esbuild` is not installed/resolvable for that module test. |
| 2026-06-18 | Phase 13C IoT guarded-module slice completed after source validation: `iot` is added to `GUARDED_PACKAGED_DATA_MODULE_IDS`, its collection resolver no longer prefers `ctx.db.raw`, the packaged guard spec registers `/modules/iot/schema.js` and smokes the module-owned `iot_widgets` collection. The inventory marks exactly `coding-agents`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `spreadsheets` and `support` as guarded-facade migrated at this checkpoint. `notes` was still open here and is closed by the immediately following smoke slice; the remaining candidates after that are `research`, `shiftflow`, `outbound`, `calendar`, `buchhaltung` and `matching`. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/iot/index.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `git diff --check -- src/apps/business-os/app.js src/apps/business-os/modules/iot/index.js src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js docs/business-os-db-isolation-inventory.json docs/business-os-roles-permissions-plan.md docs/business-os-dynamic-apps-permissions-concept.md`; accepted Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61101 SIGNALING_PORT=61102 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_module=coding-agents`, `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`, `business_os_dynamic_packaged_guard_modules=coding-agents,conversations,customers,cv-print-builder,documents,invoices,iot,spreadsheets,support`, `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,business_commands,business_commands`, `business_os_dynamic_packaged_guard_count=9`, `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard deny/grant/write keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=70`. `node src/apps/business-os/modules/iot/iot.test.mjs` was attempted separately but could not run in this workspace because `esbuild` is not installed/resolvable for that module test. |
| 2026-06-18 | Phase 13C Notes guarded-module slice completed after source validation: `notes` is added to `GUARDED_PACKAGED_DATA_MODULE_IDS`, its collection resolver no longer unwraps `ctx.db.raw` or legacy `notes_records`, and LocalStorage is no longer used as an authoritative note-data fallback when DB access is missing or denied. The packaged guard spec registers `/modules/notes/schema.js` and smokes the module-owned `notes` collection. The inventory now marks exactly `coding-agents`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `notes`, `spreadsheets` and `support` as guarded-facade migrated. `P13-CORE-DB-ISOLATION` remains open for the still source-validated cleanup candidates `research`, `shiftflow`, `outbound`, `calendar`, `buchhaltung` and `matching`. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/notes/index.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `git diff --check -- src/apps/business-os/app.js src/apps/business-os/modules/notes/index.js src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js docs/business-os-db-isolation-inventory.json docs/business-os-roles-permissions-plan.md docs/business-os-dynamic-apps-permissions-concept.md`; accepted Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61141 SIGNALING_PORT=61142 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_module=coding-agents`, `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`, `business_os_dynamic_packaged_guard_modules=coding-agents,conversations,customers,cv-print-builder,documents,invoices,iot,notes,spreadsheets,support`, `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,notes,business_commands,business_commands`, `business_os_dynamic_packaged_guard_count=10`, `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard deny/grant/write keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=141`. Two earlier 10er attempts were not accepted: one hit startup hook reload/budget, another hit a transient persisted-fixture setup miss before the packaged-guard assertions. `node src/apps/business-os/modules/notes/notes.test.mjs` was attempted separately but could not run in this workspace because `esbuild` is not installed/resolvable for that module test. |
| 2026-06-18 | Phase 13C Calendar guarded-module slice completed after source validation: `calendar` is added to `GUARDED_PACKAGED_DATA_MODULE_IDS`, its DB resolver no longer unwraps `ctx.db.raw`, all Calendar collections are resolved through the guarded facade, default seed writes are gated by `ctx.permissions.canWriteCollection`, and permission-denied reads surface as locked data access instead of falling back to broad DB handles. The packaged guard spec registers `/modules/calendar/schema.js` and smokes the module-owned `calendar_events` collection. The inventory now marks exactly `coding-agents`, `calendar`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `notes`, `spreadsheets` and `support` as guarded-facade migrated. `P13-CORE-DB-ISOLATION` remains open for `research`, `shiftflow`, `outbound`, `buchhaltung` and `matching` plus system/internal exception work. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/calendar/index.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; accepted Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61171 SIGNALING_PORT=61172 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_module=coding-agents`, `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`, `business_os_dynamic_packaged_guard_modules=coding-agents,calendar,conversations,customers,cv-print-builder,documents,invoices,iot,notes,spreadsheets,support`, `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,calendar_events,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,notes,business_commands,business_commands`, `business_os_dynamic_packaged_guard_count=11`, `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard deny/grant/write keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=45`. One earlier 11er smoke attempt passed the feature keys but was not accepted because it hit startup hook reload/budget (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=60200`). `node src/apps/business-os/modules/calendar/calendar.test.mjs` was attempted separately but could not run in this workspace because `esbuild` is not installed/resolvable for that module test. |
| 2026-06-18 | Phase 13C Outbound guarded-module slice completed after source validation: `outbound` is added to `GUARDED_PACKAGED_DATA_MODULE_IDS`, Active Outreach resolves collections through the live guarded `ctx.db` facade instead of `ctx.db.raw`, automatic default-campaign/import-repair writes are gated by `ctx.permissions.canWriteCollection`, `ctox_queue_tasks` operational status is optional/read-permission-aware instead of being part of the Outbound module grant, and the Shell fallback manifest now matches the real Outbound manifest for `outbound_skillbooks` and `outbound_letter_templates`. The packaged guard spec registers `/modules/outbound/schema.js` and smokes the module-owned `outbound_campaigns` collection. The inventory now marks exactly `coding-agents`, `calendar`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `notes`, `outbound`, `spreadsheets` and `support` as guarded-facade migrated. `P13-CORE-DB-ISOLATION` remains open for `research`, `shiftflow`, `buchhaltung` and `matching` plus system/internal exception work. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/outbound/index.js`; `node --check src/apps/business-os/modules/outbound/active-outreach.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `git diff --check -- src/apps/business-os/app.js src/apps/business-os/modules/outbound/index.js src/apps/business-os/modules/outbound/active-outreach.js src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js docs/business-os-db-isolation-inventory.json`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; accepted Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61181 SIGNALING_PORT=61182 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_module=coding-agents`, `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`, `business_os_dynamic_packaged_guard_modules=coding-agents,calendar,conversations,customers,cv-print-builder,documents,invoices,iot,notes,outbound,spreadsheets,support`, `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,calendar_events,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,notes,outbound_campaigns,business_commands,business_commands`, `business_os_dynamic_packaged_guard_count=12`, `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard deny/grant/write keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=50`. `node src/apps/business-os/modules/outbound/outbound.test.mjs` was attempted separately but could not run in this workspace because `esbuild` is not installed/resolvable for that module test. |
| 2026-06-18 | Phase 13C Research guarded-module slice completed after source validation: `research` is added to `GUARDED_PACKAGED_DATA_MODULE_IDS`, Research data reads resolve through `ctx.db.collection(name)` behind `ctx.permissions.canReadCollection`, task/run writes are gated by `ctx.permissions.canWriteCollection`, `business_commands` and `ctox_queue_tasks` remain optional operational projections, and the Fachbericht viewer now declares `documents`, `document_versions` and `document_blob_chunks` in manifest/schema while degrading without document read grants. The packaged guard spec registers `/modules/research/schema.js` and smokes the module-owned `research_tasks` collection. The inventory now marks exactly `coding-agents`, `calendar`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `notes`, `outbound`, `research`, `spreadsheets` and `support` as guarded-facade migrated. `P13-CORE-DB-ISOLATION` remains open for `shiftflow`, `buchhaltung` and `matching` plus system/internal exception work. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/research/index.js`; `node --check src/apps/business-os/modules/research/schema.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`; `python3 -m json.tool src/apps/business-os/modules/research/module.json >/dev/null`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `git diff --check -- src/apps/business-os/app.js src/apps/business-os/modules/research/index.js src/apps/business-os/modules/research/schema.js src/apps/business-os/modules/research/module.json src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js docs/business-os-db-isolation-inventory.json`; `node src/apps/business-os/modules/research/test.mjs` was attempted separately but could not run in this workspace because `esbuild` is not installed/resolvable for that module test; accepted Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61191 SIGNALING_PORT=61192 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_module=coding-agents`, `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`, `business_os_dynamic_packaged_guard_modules=coding-agents,calendar,conversations,customers,cv-print-builder,documents,invoices,iot,notes,outbound,research,spreadsheets,support`, `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,calendar_events,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,notes,outbound_campaigns,research_tasks,business_commands,business_commands`, `business_os_dynamic_packaged_guard_count=13`, `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard deny/grant/write keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=92`. |
| 2026-06-18 | Phase 13C Shiftflow guarded-module slice completed after source validation: `shiftflow` is added to `GUARDED_PACKAGED_DATA_MODULE_IDS`, startup seed writes are skipped unless all seeded planning collections have `data.write`, runtime subscriptions mount through guarded helpers, and the previous global/DOM cached DB handles are removed. Runtime shift/time-record actions still use guarded collection-property access and are represented explicitly in the inventory instead of bypassing through cached handles. The packaged guard spec registers `/modules/shiftflow/schema.js` and smokes the module-owned `planning_shifts` collection. The inventory now marks exactly `coding-agents`, `calendar`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `notes`, `outbound`, `research`, `shiftflow`, `spreadsheets` and `support` as guarded-facade migrated. `P13-CORE-DB-ISOLATION` remains open for `buchhaltung` and `matching` plus system/internal exception work. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/shiftflow/index.js`; `node --check src/apps/business-os/modules/shiftflow/schema.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `python3 -m json.tool docs/business-os-db-isolation-inventory.json >/dev/null`; `python3 -m json.tool src/apps/business-os/modules/shiftflow/module.json >/dev/null`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/modules/shiftflow/test.mjs` - 4 passed; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; accepted Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61201 SIGNALING_PORT=61202 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_module=coding-agents`, `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`, `business_os_dynamic_packaged_guard_modules=coding-agents,calendar,conversations,customers,cv-print-builder,documents,invoices,iot,notes,outbound,research,shiftflow,spreadsheets,support`, `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,calendar_events,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,notes,outbound_campaigns,research_tasks,planning_shifts,business_commands,business_commands`, `business_os_dynamic_packaged_guard_count=14`, `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard deny/grant/write keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=86`. |
| 2026-06-18 | Phase 13C Buchhaltung guarded-module slice completed after source validation: `buchhaltung` is added to `GUARDED_PACKAGED_DATA_MODULE_IDS`, all `ctx.db.raw` access and the global `window.ctoxFibuState` state export are removed, accounting collections resolve through the guarded facade, automatic chart-of-accounts/demo seed writes require exact accounting write permission, `accounting_number_series` is reconciled across schema/manifest/Shell fallback metadata, and the UI-E2E asset helper no longer stores accounting data in localStorage. The packaged guard spec registers `/modules/buchhaltung/schema.js` and smokes the module-owned `accounting_journal_entries` collection. The inventory now marks exactly `coding-agents`, `calendar`, `buchhaltung`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `notes`, `outbound`, `research`, `shiftflow`, `spreadsheets` and `support` as guarded-facade migrated. `P13-CORE-DB-ISOLATION` remains open for `matching` plus system/internal exception work. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/modules/buchhaltung/index.js`; `node --check src/apps/business-os/modules/buchhaltung/core/ui_e2e_tests.js`; `node --check src/apps/business-os/modules/buchhaltung/schema.js`; `python3 -m json.tool src/apps/business-os/modules/buchhaltung/module.json >/dev/null`; `python3 -m json.tool src/apps/business-os/modules/buchhaltung/collections.schema.json >/dev/null`; `node src/apps/business-os/modules/buchhaltung/test.js` - passed; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 11 cases; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; first 15er Browser/Rust attempt passed all feature keys but was not accepted because `browser_warning_count=1`; two later attempts were not accepted because they hit startup hook reload/budget; accepted Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61251 SIGNALING_PORT=61252 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_module=coding-agents`, `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`, `business_os_dynamic_packaged_guard_modules=coding-agents,calendar,buchhaltung,conversations,customers,cv-print-builder,documents,invoices,iot,notes,outbound,research,shiftflow,spreadsheets,support`, `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,calendar_events,accounting_journal_entries,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,notes,outbound_campaigns,research_tasks,planning_shifts,business_commands,business_commands`, `business_os_dynamic_packaged_guard_count=15`, `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard deny/grant/write keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=68`. |
| 2026-06-18 | Phase 13C Matching guarded-module slice completed after source validation: `matching` is added to `GUARDED_PACKAGED_DATA_MODULE_IDS`, `businessOsDataSource` now receives the Shell context through `setBusinessOsDatabaseContext(ctx)`, all `ctx.db.raw` injection and standalone CTOX DB bundle fallback logic are removed, UI aliases resolve `matching_requirements`, `matching_objects` and `matching_results` through `ctx.db.collection(name)`, and writes are gated through `ctx.permissions.canWriteCollection`. The packaged guard spec registers `/modules/matching/schema.js` and smokes the module-owned `matching_requirements` collection. The inventory now marks exactly `coding-agents`, `calendar`, `buchhaltung`, `conversations`, `customers`, `cv-print-builder`, `documents`, `invoices`, `iot`, `matching`, `notes`, `outbound`, `research`, `shiftflow`, `spreadsheets` and `support` as guarded-facade migrated. `P13-CORE-DB-ISOLATION` remains open for system/internal exception work and unscoped facade closure, not for user-module `matching`. | `node --check src/apps/business-os/modules/matching/index.js`; `node --check src/apps/business-os/modules/matching/ui/businessOsDataSource.js`; `node src/apps/business-os/modules/matching/test.mjs` - 3 passed; `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 11 cases; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; first 16er Browser/Rust attempt passed all feature keys but was not accepted because `startup_smoke_hook_reload_count=1` and `startup_smoke_hook_wait_ms=60234`; accepted Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61271 SIGNALING_PORT=61272 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_packaged_guard_module=coding-agents`, `business_os_dynamic_packaged_guard_collection=coding_agent_sessions`, `business_os_dynamic_packaged_guard_modules=coding-agents,calendar,buchhaltung,conversations,customers,cv-print-builder,documents,invoices,iot,notes,outbound,research,matching,shiftflow,spreadsheets,support`, `business_os_dynamic_packaged_guard_collections=coding_agent_sessions,calendar_events,accounting_journal_entries,business_commands,customer_accounts,business_commands,business_commands,accounting_invoices,iot_widgets,notes,outbound_campaigns,research_tasks,matching_requirements,planning_shifts,business_commands,business_commands`, `business_os_dynamic_packaged_guard_count=16`, `business_os_dynamic_packaged_guard_batch_coverage=1`, all packaged guard deny/grant/write keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=123`. |
| 2026-06-18 | Phase 13 system raw cleanup slice completed for Knowledge, Reports and Tickets after source validation: these system modules no longer call `ctx.db.raw` and now resolve `knowledge_items`/`knowledge_runbooks`/`knowledge_tables`, reports/release/command collections and ticket projection collections through `ctx.db.collection(name)`. The inventory marks all three as raw-free but keeps them as explicit `system-exception-pending-review` entries until their privileged cross-source operations are covered by narrow scoped policy or tested production exceptions. `P13-CORE-DB-ISOLATION` remains open for `browser`/`ctox` raw system exceptions, `creator` property/proxy access and unscoped facade closure. | `node -e "JSON.parse(require('fs').readFileSync('docs/business-os-db-isolation-inventory.json','utf8')); console.log('json ok')"`; `node --check src/apps/business-os/modules/tickets/index.js`; `node --check src/apps/business-os/modules/reports/index.js`; `node --check src/apps/business-os/modules/knowledge/index.js`; raw DB grep over `tickets`, `reports` and `knowledge` returned no hits; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; module-specific `node src/apps/business-os/modules/knowledge/test.mjs`, `node src/apps/business-os/modules/reports/test.mjs` and `node src/apps/business-os/modules/tickets/tickets-module-smoke.mjs` were attempted but blocked before execution because this checkout cannot resolve the bare `esbuild` package; Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-ui-regression SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61281 SIGNALING_PORT=61282 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_ui_opened_modules=ctox,documents,knowledge,research`, `business_os_ui_interaction_names=ctox-zoom,documents-new-drawer,knowledge-tab-runbooks,knowledge-tab-data,knowledge-tab-skill,research-new-task-modal`, `business_os_ui_secondary_opened_modules=matching,conversations,outbound,tickets,shiftflow,buchhaltung,coding-agents,app-store,browser,calendar,creator,notes,reports,spreadsheets`, `business_os_ui_secondary_interaction_names=matching-list-matrix-tabs,conversations-channel-filter,outbound-compact-view-toggle,tickets-search-status-filter,shiftflow-center-tabs,buchhaltung-nav-switch,coding-agents-settings-modal,app-store-view-scope,browser-address-refresh,calendar-new-event-drawer,creator-expert-accordion,notes-nav-filter,reports-filter-controls,spreadsheets-search-filter`, browser errors/404/request failures 0 and `startup_smoke_hook_reload_count=0`; the run is not warning-clean because the existing Notes editor path emits Chrome's contenteditable/flex advisory (`browser_warning_count=1`). |
| 2026-06-18 | Phase 13 system raw cleanup extended to Browser and CTOX after source validation: Browser now resolves `business_commands`, browser runtime collections and queue projections through `browserCollection(ctx, name)`, and CTOX now resolves runtime settings, command, queue and bug-report projections through `ctoxCollection(ctx, name)`. Direct `ctx.db.raw` and `ctx.db.collections` fallback access is gone from these two system modules. The inventory keeps Browser and CTOX as `system-exception-pending-review` because their runtime-control duties still need scoped system-policy review, but no module inventory entry now has `uses_raw_db=true`. At this checkpoint `P13-CORE-DB-ISOLATION` remained open for `creator` property/proxy access and unscoped facade closure; the follow-up Creator row closes the property/proxy part. | `node --check src/apps/business-os/modules/browser/index.js`; `node --check src/apps/business-os/modules/ctox/index.js`; raw/proxy grep over `browser` and `ctox` returned no hits; `node -e "JSON.parse(require('fs').readFileSync('docs/business-os-db-isolation-inventory.json','utf8')); console.log('json ok')"`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; final Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-ui-regression SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61301 SIGNALING_PORT=61302 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_ui_opened_modules=ctox,documents,knowledge,research`, `business_os_ui_interaction_names=ctox-zoom,documents-new-drawer,knowledge-tab-runbooks,knowledge-tab-data,knowledge-tab-skill,research-new-task-modal`, `business_os_ui_secondary_opened_modules=matching,conversations,outbound,tickets,shiftflow,buchhaltung,coding-agents,app-store,browser,calendar,creator,notes,reports,spreadsheets`, `business_os_ui_secondary_interaction_names=matching-list-matrix-tabs,conversations-channel-filter,outbound-compact-view-toggle,tickets-search-status-filter,shiftflow-center-tabs,buchhaltung-nav-switch,coding-agents-settings-modal,app-store-view-scope,browser-address-refresh,calendar-new-event-drawer,creator-expert-accordion,notes-nav-filter,reports-filter-controls,spreadsheets-search-filter`, browser errors/404/request failures 0 and `startup_smoke_hook_reload_count=0`; the run remains not warning-clean because the existing Notes editor path emits Chrome's contenteditable/flex advisory (`browser_warning_count=1`). |
| 2026-06-18 | Phase 13 Creator DB-fallback cleanup completed after source validation: Creator's own collection resolver now uses only `ctx.db.collection(name)`, and the generated app template also no longer falls back to `ctx.db.collections`. The inventory now marks Creator as raw/property/proxy/cached-handle free, while preserving its `internal-exception-pending-review` status because app-building commands, source writes and generated-app install flows still need narrow scoped policies or narrow tested production exceptions. `P13-CORE-DB-ISOLATION` remains open for unscoped facade closure and scoped system/internal policy review. | `node --check src/apps/business-os/modules/creator/index.js`; raw/proxy grep over `src/apps/business-os/modules/creator/index.js` returned no hits for `ctx.db` property/proxy access; `node -e "JSON.parse(require('fs').readFileSync('docs/business-os-db-isolation-inventory.json','utf8')); console.log('json ok')"`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`; `node src/apps/business-os/scripts/assert-module-conformance.mjs`. |
| 2026-06-18 | Phase 13G guarded-module property/proxy cleanup completed after source validation: Calendar, Coding Agents, Customers, Invoices, IoT, Notes, Support, CV Print Builder, Documents, Spreadsheets, Buchhaltung, Shiftflow and Outbound now resolve runtime collections only through `ctx.db.collection(name)` helper paths and no longer fall back to `ctx.db.<collection>` or `ctx.db.collections`. Customers import writes, document/spreadsheet persistence helpers, Shiftflow planning actions and Outbound realtime/count/update paths were moved to explicit collection handles. The inventory now reports zero module DB-access flags across all 24 modules. `P13-CORE-DB-ISOLATION` remains open for scoped system/internal policy review and unscoped facade closure, not for guarded-module property/proxy cleanup. | JS syntax checks for all touched modules passed; `node -e "JSON.parse(require('fs').readFileSync('docs/business-os-db-isolation-inventory.json','utf8')); console.log('json ok')"`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 11 cases; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 4 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; inventory flag query returned `flagged_modules=0`; available module tests passed for `buchhaltung`, `shiftflow`, `support` and `coding-agents`; other module tests were attempted but this checkout cannot resolve the bare `esbuild` package; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` completed with existing warnings after one SIGTERM-interrupted first attempt; accepted Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61325 SIGNALING_PORT=61326 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with all 16 packaged guard modules, all deny/grant/write keys true, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0`, `startup_smoke_hook_wait_ms=68`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-ui-regression SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61331 SIGNALING_PORT=61332 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK for primary/secondary Shell module interactions with browser errors/404/request failures 0 and `startup_smoke_hook_reload_count=0`; this UI-regression run remains not warning-clean because Notes triggers Chrome's contenteditable/flex advisory (`browser_warning_count=1`). |
| 2026-06-18 | Phase 13H scoped-facade read-only validation completed: four naked Shell `createLiveDbFacade()` surfaces and five Desktop-app contexts were revalidated against the current source. The plan now splits closure into Settings admin facade, per-desktop-app facades, Business Chat companion facade with actor/owner filtering and attachment-only file access, and Business Reporter facade with active-module metadata but report-system collections. No files were changed by the subagent; this row records implementation planning only. | Read-only explorer validation against `src/apps/business-os/app.js::createLiveDbFacade`, `openSettingsDrawer`, `openDesktopApp`, `scheduleBusinessCompanions`, `docs/business-os-db-isolation-inventory.json::unscoped_facades` and desktop app wrappers. Existing local evidence remains `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` and `node --check src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`. |
| 2026-06-18 | Phase 13H scoped-facade implementation completed: `src/apps/business-os/app.js` now routes Settings, Desktop app windows, Business Chat and Business Reporter through `createScopedSystemDbFacade(scopeName, collectionNames)` instead of naked `createLiveDbFacade()`. Settings, Business Chat and Business Reporter use explicit collection allowlists; Desktop apps use per-app allowlists for Code Editor, Explorer and File Viewer and empty allowlists for Browser/Creator where source search found no DB usage. The inventory now reports 0 unscoped facades. `P13-SCOPED-SYSTEM-FACADES` is closed; `P13-CORE-DB-ISOLATION` remains open only for narrow system/internal policy or tested-exception review. | `node --check src/apps/business-os/app.js`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 0 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; inventory flag query returned `flagged_modules=0`, `unscoped_facades=0`; `node --test src/apps/business-os/shared/react-settings.test.mjs` - 9 passed; `node --test src/apps/business-os/shared/business-chat.test.mjs` - 3 passed; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-agent-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61341 SIGNALING_PORT=61342 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with Business Chat scope and Settings grant-boundary evidence, browser warnings/errors/404/request failures 0; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-roles-permissions-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61343 SIGNALING_PORT=61344 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with Settings diagnostics/support evidence, browser warnings/errors/404/request failures 0; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=workspace-large-file-viewer-rust-to-browser SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61345 SIGNALING_PORT=61346 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with File Viewer rendering 1,260,036 bytes through the Desktop scoped facade, browser warnings/errors/404/request failures 0; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-ui-regression SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61347 SIGNALING_PORT=61348 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK for primary/secondary Shell module interactions with browser errors/404/request failures 0 and the known Notes contenteditable/flex browser warning. |
| 2026-06-18 | Phase 13I scoped system/internal exception implementation completed: App Store, Browser, Creator, CTOX, Desktop, Knowledge, Reports and Tickets now receive `createScopedSystemDbFacade` via `SCOPED_SYSTEM_MODULE_DB_COLLECTIONS` instead of the broad compatibility facade. The DB-isolation inventory stores exact `scoped_collections`, marks the seven system modules as `system-scoped-exception-tested` and Creator as `internal-scoped-exception-tested`, and the inventory guard validates those lists against `app.js` while failing any remaining `*-pending-review` module status. `P13-CORE-DB-ISOLATION` and Phase 13 are closed locally. | `node --check src/apps/business-os/app.js`; `node --check src/apps/business-os/scripts/assert-db-isolation-inventory.mjs`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs --self-test` - 11 cases; `node src/apps/business-os/scripts/assert-db-isolation-inventory.mjs` - 24 modules, 5 desktop apps, 0 unscoped facades; `node src/apps/business-os/scripts/assert-module-conformance.mjs` - 24 modules; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-dynamic-apps-system-scope-smoke.json BUSINESS_PORT=61731 SIGNALING_PORT=61732 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_system_scope_modules=app-store,browser,creator,ctox,desktop,knowledge,reports,tickets`, `business_os_dynamic_system_scope_count=8`, all system scope allowed/foreign/raw/permission/capability keys 1, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=48`. |
| 2026-06-18 | Phase 14B Auth Scope Browser/Rust proof completed: the Shell now preserves explicit logout over stored local pairing unless a fresh URL-pairing handoff is present, the account drawer marks the browser explicitly logged out before `/logout`, and `business-os-auth-scope-ui` proves real login, authenticated reload, logout, logged-out reload, protected-access blocking, stable local tenant/workspace scope, clean browser context, forged legacy auth storage not widening access and final logged-out state. `P14-AUTH-TENANT-SMOKE` is closed for local auth/reload/protected-access and local tenant-scope stability; hosted/multi-workspace tenant boundary and fresh-profile remain open. | `node --check src/apps/business-os/app.js`; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `git diff --check -- src/apps/business-os/app.js src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js src/core/rxdb/tools/business_os_production_smoke_registry.js`; final Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-auth-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60721 SIGNALING_PORT=60722 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_auth_login_verified=1`, `business_os_auth_authenticated_reload_verified=1`, `business_os_auth_logout_verified=1`, `business_os_auth_logged_out_reload_blocked=1`, `business_os_auth_protected_access_blocked=1`, `business_os_auth_tenant_scope_verified=1`, `business_os_auth_browser_context_clean=1`, `business_os_auth_storage_copy_did_not_widen_scope=1`, `business_os_auth_final_state=logged_out`, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=55`. |
| 2026-06-18 | Phase 14D/14E Fresh Profile Browser/Rust proof completed: `business-os-fresh-profile-ui` now records an empty Chromium profile before launch, then proves authoritative Business OS projection, Shell lifecycle labels, version labels, lifecycle drawer labels, App Store disabled reasons, desktop viewport, narrow viewport and browser-storage non-authority through the real Shell/App Store render path. `P14-VISUAL-LABEL-QA` is closed and local fresh-profile storage-boundary evidence is complete; hosted/multi-workspace tenant boundary and scale budgets remain open. | `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `git diff --check -- src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js src/core/rxdb/tools/business_os_production_smoke_registry.js`; final Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-fresh-profile-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60751 SIGNALING_PORT=60752 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_fresh_profile_clean_indexeddb=1`, `business_os_fresh_profile_clean_local_storage=1`, `business_os_fresh_profile_clean_session_storage=1`, `business_os_fresh_profile_authoritative_projection_loaded=1`, `business_os_fresh_profile_lifecycle_labels_visible=1`, `business_os_fresh_profile_version_badges_visible=1`, `business_os_fresh_profile_disabled_reasons_visible=1`, `business_os_fresh_profile_desktop_viewport_verified=1`, `business_os_fresh_profile_narrow_viewport_verified=1`, `business_os_fresh_profile_no_storage_widening=1`, `business_os_fresh_profile_auth_state=authenticated`, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=48`. |
| 2026-06-18 | Phase 14F tenant-boundary and scale-budget closure completed: Auth Scope now injects forged stored pairing/auth data from a different scope and requires `business_os_auth_cross_scope_storage_denied=1` plus `business_os_auth_tenant_scope_claim=local-workspace-only`, making the local-only tenant claim explicit. Fresh Profile now includes a representative scale fixture with 32 extra runtime apps, 64 explicit grants, 96 native module-version rows and 128 native audit events; the production registry supports `maximums` and enforces render/start-menu/App-Store budget ceilings. `P14-TENANT-BOUNDARY`, `P14-PERF-SCALE-BUDGET` and Phase 14 are closed for the current local-workspace product claim; hosted/multi-workspace isolation remains future hosted-product scope and is not claimed. | `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; Auth Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-auth-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-auth-tenant-boundary-smoke.json BUSINESS_PORT=61741 SIGNALING_PORT=61742 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with cross-scope denial and local-only tenant claim; Fresh Profile Scale Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-fresh-profile-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-fresh-profile-scale-smoke.json BUSINESS_PORT=61751 SIGNALING_PORT=61752 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with 57 catalog apps, 64 explicit grants, 96 release versions, 128 native audit events, 32 App Store cards, render 8 ms, start menu 10 ms, App Store 116 ms, browser warnings/errors/404/request failures 0 and `startup_smoke_hook_wait_ms=36`; full production Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES="$(node -e "const { businessOsProductionSmokeModes } = require('./src/core/rxdb/tools/business_os_production_smoke_registry'); process.stdout.write(businessOsProductionSmokeModes.join(','));")" SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_BROWSER_WARNING_BUDGET=0 SMOKE_BROWSER_REQUEST_FAILURE_BUDGET=0 SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-production-smoke-phase14-scale-summary.json BUSINESS_PORT=61761 SIGNALING_PORT=61762 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK for Release, Audience, Agent Scope, Auth Scope and Fresh Profile with browser warnings/request failures/startup reloads 0. |
| 2026-06-18 | Phase 15A native "Warum?" diagnostics command slice completed: `ctox.business_os.why` now returns native lifecycle/policy diagnostics for app visibility/open/edit/source/release/rollback and per-data-area data read/write decisions. The command uses the native catalog projection plus central policy engine, preserves the browser rule that Owner/Admin authority does not make private `0.x` drafts team-visible, and stores a sanitized command projection for this diagnostics command. A small existing compile blocker in `src/core/service/service.rs` was also fixed: one stale `format!` argument had no matching placeholder. `P15-WHY-DIAGNOSTICS` remains open for Settings/support UI parity and stable support artifact schema/export. | `rustfmt --edition 2021 --check src/core/business_os/store.rs src/core/service/service.rs`; `cargo test --bin ctox business_os_why_command --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 2 passed, 0 failed, 1790 filtered out; `cargo test --bin ctox business_os_app_ --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 20 passed, 0 failed, 1772 filtered out; existing warnings only. |
| 2026-06-18 | Phase 15A Settings "Warum?" diagnostics static UI slice completed: module-management rows now expose a business-facing `Warum?` action that dispatches `ctox.business_os.why`, renders the native actor/app/action/data diagnostics through the shared diagnostics component, and verifies that raw policy keys, reason codes, nested decision payloads, prompt/token/selection markers and stale Settings release/rollback dispatch paths are not exposed. `P15-WHY-DIAGNOSTICS` remains open for live Settings Browser/Rust proof and support artifact schema/export. | `node --check src/apps/business-os/shared/react-settings.js`; `node --test src/apps/business-os/shared/react-settings.test.mjs` - 8 passed, 0 failed |
| 2026-06-18 | Phase 15A live Settings "Warum?" Browser/Rust proof completed and accepted clean: `business-os-roles-permissions-ui` clicks the Settings module-management `Warum?` action, waits for the native `ctox.business_os.why` projection result, requires actor/visibility/open/modify/source/release/rollback/data rows, verifies no raw policy keys/reason codes/prompt/token/selection markers render, and keeps browser warnings/errors/404/request failures at 0. The same slice hardens Desktop icon repaint during reload so expected IndexedDB-closing aborts no longer surface as browser errors. `P15-WHY-DIAGNOSTICS` remains open only for stable support artifact schema/export. | `node --check src/apps/business-os/modules/desktop/index.js`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; first Browser/Rust proof passed the new diagnostics keys but failed startup budget (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=60388`) before rerun; next pass exposed one transient Desktop IndexedDB-closing browser error, which was fixed; final Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-roles-permissions-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60961 SIGNALING_PORT=60962 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_roles_permissions_settings_why_diagnostics_visible=1`, `business_os_roles_permissions_settings_why_diagnostics_rows=1`, `business_os_roles_permissions_settings_why_diagnostics_redacted=1`, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=93`. |
| 2026-06-18 | Phase 15F native support diagnostics artifact slice completed: `ctox.business_os.support.export_diagnostics` returns a stable `ctox.business_os.support_diagnostics.v1` support-safe artifact containing actor/scope, redaction manifest, Activity summaries without raw payloads and optional sanitized Why summary. The command is gated by `users.manage`, persists a sanitized command projection and rejects prompt/token/selected-text/message-body/record-payload leakage in Rust. `P15-WHY-DIAGNOSTICS` is closed; `P15-SUPPORT-ARTIFACT-SCHEMA` remains open at this historical checkpoint for browser diagnostics export smoke and support attachment/download UX; native audit retention was later split into `P15-OPS-RECOVERY`. | `rustfmt --edition 2021 src/core/business_os/store.rs src/core/service/service.rs`; `cargo test --bin ctox business_os_support_diagnostics_export --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 1 passed, 0 failed, 1792 filtered out; existing warnings only. |
| 2026-06-18 | Phase 15F Settings support diagnostics export completed end-to-end: module-management rows expose a per-app `Support-Paket` action, dispatch the native `ctox.business_os.support.export_diagnostics` command, render only business-facing schema/protection/scope/activity/why rows, and provide a JSON download link without showing raw policy keys, record payload names, prompt/token/selection markers or command internals. The Browser/Rust roles-permissions smoke now seeds a real native Owner actor for this flow so the `users.manage` gate stays production-faithful instead of trusting browser-only state. `P15-SUPPORT-ARTIFACT-SCHEMA` is closed; native audit export-before-prune was still a separate Phase 15B/`P15-OPS-RECOVERY` item at this historical checkpoint and is addressed by the later `ctox.business_os.audit.retention` slice. | `node --check src/apps/business-os/shared/react-settings.js`; `node --test src/apps/business-os/shared/react-settings.test.mjs` - 9 passed, 0 failed; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings after fixing a pre-existing `format!` argument mismatch in `src/core/service/service.rs`; final Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-roles-permissions-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60991 SIGNALING_PORT=60992 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_roles_permissions_settings_support_diagnostics_visible=1`, `business_os_roles_permissions_settings_support_diagnostics_rows=1`, `business_os_roles_permissions_settings_support_diagnostics_redacted=1`, `business_os_roles_permissions_settings_support_diagnostics_download=1`, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=83`. |
| 2026-06-18 | Phase 15A Shell "Warum?" diagnostics UI slice completed: the lifecycle drawer now renders a shared business-facing diagnostics model explaining actor, app visibility/open/edit/source/release/rollback and per-data-area read/write decisions. The Dynamic Apps Browser/Rust smoke now requires stable `data-why-*` rows for manager and read-only drawer states. `P15-WHY-DIAGNOSTICS` remains open because native diagnostics command output/tests and Settings/support diagnostics parity are still required. | `node --check src/apps/business-os/shared/shell-permissions-ui.js`; `node --check src/apps/business-os/app.js`; `node --test src/apps/business-os/shared/shell-permissions-ui.test.mjs` - 9 passed; `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; final Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-dynamic-apps-ui SMOKE_MATRIX_ATTEMPTS=2 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=60851 SIGNALING_PORT=60852 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with `business_os_dynamic_lifecycle_why_diagnostics_visible=1`, `business_os_dynamic_lifecycle_why_diagnostics_rows=1`, `business_os_dynamic_lifecycle_why_diagnostics_data=1`, browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0` and `startup_smoke_hook_wait_ms=38`. |
| 2026-06-18 | Phase 16B smoke artifact schema completed locally: the Browser/Rust smoke matrix now writes `runtime/build/business-os-smoke-matrix-summary.json` by default and validates schema `ctox.business_os.smoke_matrix_summary.v1` before writing a successful summary. The artifact contains git revision, URL, mode, auth state, actor role, browser context, tenant scope, evidence keys, full evidence, warning budgets and pass/fail result. `P16-SMOKE-ARTIFACT` is closed for local schema/default-path evidence; CI upload/retention, clean bootstrap, release workflow gating and docs/signoff remain open Phase 16 work. | `node --check src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - includes negative checks for missing git revision, missing attempt URL, missing configuration and evidence-key drift; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings; first `business-os-agent-scope-ui` run with explicit result path failed startup budget (`startup_smoke_hook_reload_count=1`, `startup_smoke_hook_wait_ms=61751`) and was not accepted; final default-path Browser/Rust `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-agent-scope-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 BUSINESS_PORT=61441 SIGNALING_PORT=61442 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with browser warnings/errors/404/request failures 0, `startup_smoke_hook_reload_count=0`, `startup_smoke_hook_wait_ms=81`; artifact JSON validation confirmed fixed absolute result path, git revision `3c33b8cbc24ae08fd31be857a2fecae0519cb83c`, URL `http://127.0.0.1:61441/index.html`, authenticated user clean local-workspace context, 36 evidence keys and warning-budget fields. |
| 2026-06-18 | Phase 16A clean-checkout JS/bootstrap completed locally and wired into CI: Business OS now declares its test-only Node dependencies in `src/apps/business-os/package.json`/`package-lock.json`, pins audit-clean `esbuild@0.28.1` and `playwright@1.60.0`, verifies package and lock pins through `assert-business-os-js-bootstrap.mjs`, and lets the Browser/Rust smoke runner resolve Playwright from `src/apps/business-os/node_modules/playwright` without a host-global install. The x86_64 Linux CI job now runs the Business OS npm bootstrap, audit, shared/App Store tests and esbuild-backed module bundle tests. `P16-JS-BOOTSTRAP` is closed; full production release gating remains open in `P16-CI-RELEASE-GATES`/`P16-RELEASE-WORKFLOW-GATE`. | `npm ci --ignore-scripts --prefix src/apps/business-os` - added 5 packages, 0 vulnerabilities; `npm audit --audit-level=low --prefix src/apps/business-os` - 0 vulnerabilities; `npm --prefix src/apps/business-os run check:deps` - `business_os_js_bootstrap=1`, esbuild 0.28.1, Playwright 1.60.0; `npm --prefix src/apps/business-os test` - 48 shared tests and 18 App Store tests passed; `npm --prefix src/apps/business-os run test:module-bundles` - Calendar 7 tests, CTOX 9 checks, Customers schema smoke, IoT contract, Notes 4 tests, Outbound 10 tests and Research 5 tests passed; `node --check src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js src/apps/business-os/modules/ctox/test.js src/apps/business-os/scripts/assert-business-os-js-bootstrap.mjs`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `git diff --check -- .github/workflows/ci.yml src/apps/business-os/package.json src/apps/business-os/package-lock.json src/apps/business-os/modules/ctox/test.js src/apps/business-os/scripts/assert-business-os-js-bootstrap.mjs src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js`. |
| 2026-06-18 | Phase 16C/16D production CI and release workflow gates completed locally and wired into GitHub Actions: CI now runs the declared Business OS JS bootstrap and warning-clean production Browser/Rust smoke matrix with fixed artifact upload; the tag-release workflow now has a `business-os-production-gate` job and all artifact-producing release jobs depend on it. The smoke runner now treats known data-plane-shutdown catalog refreshes as debug-only and Auth/Fresh-Profile evidence includes auth state, actor role, browser context and tenant scope so every production mode has a complete artifact context. The artifact validator now rejects final production summaries with missing context values or accepted attempts above their recorded budgets. `P16-CI-RELEASE-GATES` and `P16-RELEASE-WORKFLOW-GATE` are closed; later 16E and 16G rows close the legacy fixture and docs dry-run gaps, so current remaining Phase 16 work is security/privacy signoff and final customer/operator release review. | `npm ci --ignore-scripts --prefix src/apps/business-os`; `npm audit --audit-level=low --prefix src/apps/business-os`; `npm --prefix src/apps/business-os test`; `npm --prefix src/apps/business-os run test:module-bundles`; `cargo build --locked --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`; `node --check src/apps/business-os/app.js src/core/rxdb/tools/browser_rust_smoke.js src/core/rxdb/tools/browser_rust_smoke_matrix.js src/core/rxdb/tools/business_os_production_smoke_registry.js src/apps/business-os/scripts/assert-business-os-js-bootstrap.mjs src/apps/business-os/modules/ctox/test.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; release workflow Ruby dependency guard returned `business_os_release_gate_upload_dependency=1`; final warning-clean Browser/Rust `SMOKE_MODES="$(node -e "const { businessOsProductionSmokeModes } = require('./src/core/rxdb/tools/business_os_production_smoke_registry'); process.stdout.write(businessOsProductionSmokeModes.join(','));")" SMOKE_MATRIX_ATTEMPTS=2 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_BROWSER_WARNING_BUDGET=0 SMOKE_BROWSER_REQUEST_FAILURE_BUDGET=0 BUSINESS_PORT=61691 SIGNALING_PORT=61692 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` passed Release, Audience, Agent Scope, Auth Scope and Fresh Profile with browser warnings/request failures/startup reloads 0; artifact validation confirmed schema, git revision `136ac5bd50de646d9684f7d2f57c76aa30e4bad3`, attempts 2, all five URLs and complete context fields. |
| 2026-06-18 | Phase 16F/16G release-signoff and customer/operator-doc structure added without claiming final signoff: `docs/business-os-security-privacy-signoff.json` now defines the blocking machine-readable `ctox.business_os.security_privacy_signoff.v1` controls and source-hash slots for dynamic app runtime, source visibility, data review/locked state, MCP/agent scope, audit/support export, external effects and artifact integrity. `docs/business-os-production-release-signoff.md` is the human checklist companion. `docs/business-os-app-access-and-roles-guide.md` and `docs/business-os-roles-permissions-operator-guide.md` document current customer/operator behavior without inventing future controls: Settings release/rollback stays read-only diagnostics, private `0.x` visibility is not automatic for Owner/Admin, and app visibility stays separate from data/source/edit/release rights. CI validates doc/signoff structure, while the tag-release workflow runs the Security/Privacy guard with `--require-signed-off`, intentionally blocking release artifacts while the JSON signoff is pending. The later 16G row adds UI-source dry-run evidence; actual security/privacy signoff and final human release review remain open. | `node --check src/apps/business-os/scripts/assert-production-release-docs.mjs`; `node --check src/apps/business-os/scripts/assert-security-privacy-signoff.mjs`; `node src/apps/business-os/scripts/assert-production-release-docs.mjs` - `business_os_release_docs_ok=1 status=pending-signoff`; `node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs --self-test`; `node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs` - `business_os_security_privacy_signoff_ok=1 require_signed_off=0 status=pending-signoff`; `node src/apps/business-os/scripts/assert-security-privacy-signoff.mjs --require-signed-off` - failed as expected on pending controls, pending status, missing reviewer/date/evidence revision and missing source hashes, and wrote `runtime/build/business-os-security-privacy-signoff-validation.json` with `ok=false`; `git diff --check -- .github/workflows/ci.yml .github/workflows/release.yml src/apps/business-os/package.json src/apps/business-os/scripts/assert-production-release-docs.mjs src/apps/business-os/scripts/assert-security-privacy-signoff.mjs docs/business-os-roles-permissions-rollout.md docs/business-os-app-access-and-roles-guide.md docs/business-os-roles-permissions-operator-guide.md docs/business-os-production-release-signoff.md docs/business-os-security-privacy-signoff.json`. |
| 2026-06-18 | Phase 16E legacy migration fixture coverage completed: `module_catalog_projects_runtime_app_lifecycle_backfill` now includes private `0.x`, missing version, invalid SemVer, released `1.x`, restricted and preview legacy manifests, plus a pre-existing partial grant to prove idempotency. The test asserts preview/restricted legacy manifests get only the expected `apps.view` grants, released/missing/invalid/private apps do not widen visibility, no `data.read` or `data.write` grants are created, invalid SemVer stays private with `current_semver=null`, and rerunning `module_catalog_for_rxdb` does not duplicate grants. `P16-LEGACY-FIXTURES` is closed; Phase 16 remains open for actual security/privacy signoff and final customer/operator release review. | `rustfmt --edition 2021 --check src/core/business_os/store.rs`; `cargo test --bin ctox module_catalog_projects_runtime_app_lifecycle_backfill --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 1 passed; `cargo test --bin ctox business_app_semver_major_matches_browser_plain_semver_contract --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 1 passed; `node --test src/apps/business-os/shared/app-lifecycle.test.mjs` - 13 passed. |
| 2026-06-18 | Phase 16G release docs dry-run evidence added: `assert-production-release-docs.mjs` now writes `runtime/build/business-os-release-docs-dry-run.json` with schema `ctox.business_os.release_docs_dry_run.v1`, doc paths, current signoff status, UI-source evidence for lifecycle badges, app governance actions, App Store release flow, Settings Warum/Support-Paket, shell diagnostics and production smoke registry anchors, plus smoke-summary metadata when available. The release workflow reruns the guard after the Browser/Rust production gate and uploads the dry-run artifact, customer guide and operator guide with the production evidence bundle. `P16-CUSTOMER-OPERATOR-DOCS` remains open only for final human release review/signoff. | `node --check src/apps/business-os/scripts/assert-production-release-docs.mjs`; `node src/apps/business-os/scripts/assert-production-release-docs.mjs` - wrote `runtime/build/business-os-release-docs-dry-run.json`; artifact spot-check confirmed `ok=true`, all UI-source evidence checks passed and smoke summary was linked when present. |
| 2026-06-18 | Phase 15B MCP retention boundary closed: MCP policy now persists in typed Business OS payload `business_os.mcp_policy.v1`, `ctox business-os mcp policy set` writes that typed state, and `CTOX_BUSINESS_OS_MCP_*` runtime-env keys are kept only as migration fallback until typed policy exists. MCP audit pruning now reads typed `audit_retention_days`; native `business_events` export-before-prune retention was still open at this historical checkpoint and is addressed by the following native `ctox.business_os.audit.retention` slice. Recovery commands/drills and backup/restore drills remain open Phase 15 work. | `rustfmt --edition 2021 --check src/core/business_os/mcp_channel.rs src/core/service/business_os.rs`; `cargo test --bin ctox mcp_policy --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 11 passed; `cargo test --bin ctox audit_retention_prunes_expired_mcp_events --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 1 passed; `cargo test --bin ctox mcp_business_os_policy --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 18 passed. |
| 2026-06-18 | Phase 15B native `business_events` export-before-prune slice added: `ctox.business_os.audit.retention` now exports expired native audit rows as support-safe `ctox.business_os.audit_retention_export.v1` JSON under `runtime/business-os/audit-exports` before optional prune, gates the command with `users.manage`, stores a sanitized command projection and denies spoofed Teammitglied attempts without creating an export. `P15-OPS-RECOVERY` moves to partial/in-progress; recovery commands/drills and backup/restore remain open. | `rustfmt --edition 2021 --check src/core/business_os/store.rs src/core/business_os/mcp_channel.rs src/core/service/business_os.rs`; `cargo test --bin ctox audit_retention_command --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 2 passed; `cargo test --bin ctox audit_list_command --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 2 passed; `cargo test --bin ctox business_os_support_diagnostics_export --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 1 passed; `cargo test --bin ctox business_event_audit --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 8 passed; `git diff --check -- src/core/business_os/store.rs src/core/business_os/mcp_channel.rs src/core/service/business_os.rs docs/business-os-roles-permissions-plan.md docs/business-os-roles-permissions-rollout.md docs/business-os-dynamic-apps-permissions-concept.md`. |
| 2026-06-18 | Phase 15C lifecycle projection recovery drill slice added: `ctox.module.repair_lifecycle_projection` now accepts `dry_run`, returns planned repair actions, avoids `business_records`/RxDB/catalog mutation in dry-run, uses sanitized command projection for persisted `business_commands`, and still applies the existing release/catalog projection repair when dry-run is false. `P15-REPAIR-COMMANDS` moves to partial/in-progress; broader repair coverage remains open. | `rustfmt --edition 2021 src/core/business_os/store.rs`; `cargo test --bin ctox module_lifecycle_projection_repair --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 1 passed. |
| 2026-06-18 | Phase 15E native isolated backup/restore drill slice added: `ctox business-os backup restore-drill [--module <id>]` creates online SQLite snapshots for core, Business OS and native RxDB stores, copies installed modules, source snapshots and audit exports, restores into an isolated root, validates SQLite integrity, writes a hash manifest and runs support-safe restored-state validation for typed MCP policy, release rows, rollback target, installed manifests, source snapshots, audit exports and RxDB catalog projection. The native `ctox.business_os.backup.restore_drill` command is a `runtime.manage` gated support-safe preflight with sanitized command projection and does not write raw backups. `P15-BACKUP-RESTORE-DRILL` moves to partial/in-progress; active-root restore/quiesce/restart, browser IndexedDB unsynced local state, hosted WebRTC resync proof, cross-version restore and raw backup encryption/signing/retention remain open. | `rustfmt --edition 2021 src/core/business_os/store.rs src/core/service/business_os.rs`; `cargo check --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target`; `cargo test --bin ctox backup_restore_drill --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 2 passed; `cargo test --bin ctox audit_retention_command --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 2 passed; `cargo test --bin ctox module_lifecycle_projection_repair --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 1 passed; `cargo test --bin ctox mcp_policy_set_persists_typed_policy_state --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 1 passed. |
| 2026-06-18 | Phase 15G stale module-grant repair slice added: `ctox.module.repair_lifecycle_projection` now accepts `repair_stale_grants` and reports/deactivates active module-scoped permission grants whose `scope_id` no longer matches a current module manifest. The dry-run/action model stays support-safe and does not mutate grants; apply deactivates only the stale grant with repair reason and leaves valid module grants active. At this checkpoint `P15-REPAIR-COMMANDS` still needed private-app responsibility recovery, manifest/source divergence boundaries and invalid release-version-ref recovery later covered by 15J/15K. | `rustfmt --edition 2021 --check src/core/business_os/store.rs`; `cargo test --bin ctox module_lifecycle_projection_repair --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 2 passed; `git diff --check -- src/core/business_os/store.rs docs/business-os-dynamic-apps-permissions-concept.md docs/business-os-roles-permissions-plan.md`. |
| 2026-06-18 | Phase 15H native audit-retention policy slice added: `ctox.business_os.audit.retention_policy.set` persists validated native retention days as `business_os.audit_retention_policy.v1`, is gated by `users.manage`, stores a sanitized command projection and is consumed by `ctox.business_os.audit.retention` when the request omits `retention_days`. Restore-drill service-state surfaces now prove typed native audit-retention policy presence/validity and effective days next to typed MCP policy. `P15-OPS-RECOVERY` is closed for audit/retention/support evidence; Phase 15 still remains open for active-root/browser/cross-version/raw-backup boundaries. | `rustfmt --edition 2021 --check src/core/business_os/store.rs`; `cargo test --bin ctox audit_retention --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 5 passed; `cargo test --bin ctox backup_restore_drill --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 2 passed; `cargo test --bin ctox module_lifecycle_projection_repair --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 2 passed; `git diff --check -- src/core/business_os/store.rs docs/business-os-dynamic-apps-permissions-concept.md docs/business-os-roles-permissions-plan.md`. |
| 2026-06-18 | Phase 15I restore-boundary evidence tightened: the isolated backup/restore drill test now asserts that browser IndexedDB unsynced local state remains listed in `remaining_boundaries`. This keeps the production plan honest: native store/module/source/audit restore evidence exists, but browser-local unsynced state still needs a separate active-root/browser/WebRTC recovery proof before full production-ready signoff. | `rustfmt --edition 2021 --check src/core/business_os/store.rs`; `cargo test --bin ctox backup_restore_drill --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 2 passed. |
| 2026-06-18 | Phase 15J invalid release-version-ref repair slice added: `ctox.module.repair_lifecycle_projection` now accepts `repair_invalid_version_refs`, reports release snapshot `source_version_id`/`rollback_version_id` values whose referenced module source version no longer exists, and in apply mode clears only those broken fields before regenerating the release projection. Dry-run leaves snapshot JSON unchanged, the persisted repair command projection is sanitized, and valid release metadata remains under the normal release preflight guard. At this checkpoint `P15-REPAIR-COMMANDS` still needed orphan private-app and source-file restore runbooks; orphan responsibility recovery was later covered by 15K. | `rustfmt --edition 2021 src/core/business_os/store.rs`; `cargo test --bin ctox module_lifecycle_projection_repair --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 3 passed. |
| 2026-06-18 | Phase 15K orphan private-app recovery slice added: `ctox.module.repair_lifecycle_projection` now accepts `repair_orphan_private_apps`, detects private runtime `0.x` apps with no active App-Verantwortliche after legacy/restore drift, reports the intended recovery assignment in dry-run, and in apply mode assigns the current Admin/Owner actor through the existing audited `business_module_acl` path. The test proves dry-run has no ACL mutation, apply writes responsibility, emits `business_os.app_responsibility.changed`, sanitizes persisted command projection and refreshes catalog lifecycle responsibility. `P15-REPAIR-COMMANDS` remains open only for manifest/source file restore beyond invalid metadata refs. | `rustfmt --edition 2021 src/core/business_os/store.rs`; `cargo test --bin ctox module_lifecycle_projection_repair --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 4 passed. |
| 2026-06-18 | Phase 15L source-version manifest restore evidence tightened: `module_versions_record_rollback_and_remove_added_files` now mutates `module.json` in addition to editable source files and a post-target added file, then proves `ctox.module.rollback_version`/`rollback_module_to_version` restores the baseline manifest, restores source content, removes the added file and records a sealed rollback boundary. `P15-REPAIR-COMMANDS` moves to Complete; recovery without any source-version history remains a backup/restore concern in `P15-BACKUP-RESTORE-DRILL`. | `rustfmt --edition 2021 src/core/business_os/store.rs`; `cargo test --bin ctox module_versions_record_rollback_and_remove_added_files --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 1 passed. |
| 2026-06-18 | Phase 15M active-root restore runbook evidence added: the native backup/restore drill and support-safe preflight now embed `business_os_active_root_restore_runbook` with `destructive_restore_performed=false`, quiesce/restart gates, manifest SHA verification and restore targets for core/Business OS/native RxDB SQLite stores, installed app roots, source snapshots and audit exports. The focused drill test asserts the runbook is present in both CLI and preflight artifacts and that active-root restore is no longer an implicit remaining boundary, while browser IndexedDB and hosted WebRTC stay visible as drill-local boundaries. | `rustfmt --edition 2021 src/core/business_os/store.rs`; `cargo test --bin ctox backup_restore_drill --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 2 passed; `cargo build --bin ctox --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - completed with existing warnings. |
| 2026-06-18 | Phase 15N local browser restore/resync proof added: `business-os-restore-resync-ui` is now a registered production Browser/Rust smoke with fixed evidence requirements. The mode starts from a clean authenticated browser context, verifies WebRTC-only/no HTTP bridge, stops the native peer, writes a browser-local desktop file and chunk, proves native SQLite does not see the write while the peer is stopped, restarts the peer, requires fresh WebRTC checkpoint epochs, then proves the same record converges to native SQLite. The smoke preloads module scripts before the intentional peer outage so the test measures sync recovery rather than static asset availability. `P15-BACKUP-RESTORE-DRILL` is closed for the local same-profile browser IndexedDB/WebRTC-peer-restart case; hosted/multi-workspace restore, cross-version/downgrade and raw backup encryption/signing/retention remain open. | `node --check src/core/rxdb/tools/browser_rust_smoke.js`; `node --check src/core/rxdb/tools/business_os_production_smoke_registry.js`; `SMOKE_MATRIX_SELF_TEST=1 node src/core/rxdb/tools/browser_rust_smoke_matrix.js`; `PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES=business-os-restore-resync-ui SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_BROWSER_WARNING_BUDGET=0 SMOKE_BROWSER_REQUEST_FAILURE_BUDGET=0 BUSINESS_PORT=61921 SIGNALING_PORT=61922 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - matrix OK with warnings/errors/request failures 0, `business_os_restore_resync_webrtc_only=1`, `business_os_restore_resync_local_only_before_restart=1`, `business_os_restore_resync_checkpoint_epoch_count=11`, `business_os_restore_resync_native_converged_after_restart=1`; full six-mode production matrix `SMOKE_MATRIX_ATTEMPTS=2` on ports 61951/61952 passed all six production modes warning-clean with schema-validated summary artifact, `requestedAttempts=2`, complete auth/role/browser/tenant context and warning/request/asset/startup budgets 0 for the accepted attempts. |
| 2026-06-18 | Phase 15O backup manifest integrity and raw-backup policy slice added: `run_business_os_backup_restore_drill` now creates/reuses a CTOX Secret Store `business-os/backup_manifest_signing_key_v1`, snapshots `runtime/ctox-secrets.sqlite3` with the other restore-critical SQLite stores, writes `raw_backup_security` retention/support-attachment policy, writes `restore_compatibility` with same-version support and downgrade/cross-version block policy, and signs the snapshot manifest with HMAC-SHA256 before writing the final manifest hash. `business_os_active_root_restore_runbook` now requires manifest signature verification, compatibility verification, Secret Store restore target and raw-backup handling gates. `ctox business-os backup prune-drills [--dry-run]` now reports and deletes only expired `business-os-drill-*` directories with manifest retention metadata; missing retention metadata is reported but not deleted. The focused restore-drill tests assert the signed manifest, signing-key redaction, Secret Store snapshot, runbook gates, compatibility/retention fields and prune dry-run/apply behavior. Remaining backup blockers are narrowed to hosted/multi-workspace WebRTC restore proof, release-level cross-version/downgrade evidence and portable/off-machine raw-backup encryption. | `rustfmt --edition 2021 --check src/core/business_os/store.rs src/core/service/business_os.rs src/core/secrets.rs`; `cargo test --bin ctox backup_restore_drill --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 3 passed with existing unrelated warnings. |
| 2026-06-18 | Phase 15P portable encrypted backup export slice added: `run_business_os_backup_restore_drill` now creates a chunked AES-256-GCM portable snapshot ZIP export under the drill directory, using a separate Secret-Store-backed `business-os/portable_backup_encryption_key_v1`. The signed manifest records `portable_encrypted_export`, ciphertext path/SHA-256, chunk framing, nonce base, key metadata without secret value, off-machine transfer conditions and external key-escrow requirement. The drill immediately decrypts the ciphertext, verifies plaintext size/SHA-256, opens the ZIP, matches ZIP entries against the snapshot manifest and deletes temporary plaintext/verify ZIPs. `raw_backup_security` now reports `portable_encrypted_export_created` instead of leaving off-machine encryption as open, while `remaining_boundaries` keep hosted/multi-workspace restore, cross-version/downgrade evidence and key escrow as operator/release gates. | `rustfmt --edition 2021 src/core/business_os/store.rs src/core/service/business_os.rs src/core/secrets.rs`; `cargo test --bin ctox backup_restore_drill --no-default-features --target-dir runtime/build/core-rxdb-integration-target` - 3 passed with existing unrelated warnings. |
| 2026-06-18 | Phase 15Q manifest restore preflight slice added: `ctox business-os backup inspect-manifest --manifest <path>` now exposes a non-destructive preflight over a snapshot manifest. It verifies the HMAC signature with the existing Secret Store signing key without generating a new key, checks supported manifest schema, allows only same-version restore, blocks automatic cross-version and downgrade restore, and verifies the encrypted portable artifact hash. The focused backup tests now assert the preflight on a generated manifest plus explicit same-version, older-version and newer-version compatibility decisions. | `rustfmt --edition 2021 --check src/core/business_os/store.rs src/core/service/business_os.rs src/core/secrets.rs`; `node src/apps/business-os/scripts/assert-production-release-docs.mjs` - `business_os_release_docs_ok=1 status=pending-signoff`; `git diff --check -- src/core/business_os/store.rs src/core/service/business_os.rs src/core/secrets.rs docs/business-os-roles-permissions-plan.md docs/business-os-roles-permissions-operator-guide.md docs/business-os-roles-permissions-rollout.md docs/business-os-dynamic-apps-permissions-concept.md`; `CARGO_BUILD_JOBS=2 cargo test --bin ctox backup_ --no-default-features --target-dir runtime/build/business-os-backup-check-target` - 9 passed with existing unrelated warnings; `CARGO_BUILD_JOBS=2 cargo build --bin ctox --no-default-features --target-dir runtime/build/business-os-backup-check-target` - completed with existing unrelated warnings; `CTOX_BIN=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/business-os-backup-check-target/debug/ctox PLAYWRIGHT_MODULE_PATH=/Users/michaelwelsch/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/.pnpm/playwright@1.60.0/node_modules/playwright/index.js SMOKE_MODES="$(node -e "const { businessOsProductionSmokeModes } = require('./src/core/rxdb/tools/business_os_production_smoke_registry'); process.stdout.write(businessOsProductionSmokeModes.join(','));")" SMOKE_MATRIX_ATTEMPTS=1 SMOKE_PAGE_PATH=/index.html SMOKE_MODE_TIMEOUT_MS=300000 SMOKE_BROWSER_WARNING_BUDGET=0 SMOKE_BROWSER_REQUEST_FAILURE_BUDGET=0 SMOKE_MATRIX_RESULT_PATH=runtime/build/business-os-production-smoke-post-backup-preflight.json BUSINESS_PORT=62011 SIGNALING_PORT=62012 node src/core/rxdb/tools/browser_rust_smoke_matrix.js` - all six production modes passed with browser warnings/errors/404/request failures 0 and schema `ctox.business_os.smoke_matrix_summary.v1` `ok=true`. Two earlier Rust attempts were infrastructure failures from local `No space left on device`; older generated `runtime/backups/update-*` backups were pruned while retaining the latest five before rerunning. |
| 2026-06-18 | Phase 15R backup key-escrow status and ZIP64 real-root drill slice added: `ctox business-os backup key-escrow-status` now exposes a redacted, machine-readable external-escrow readiness report for the portable backup encryption key. The command reads the CTOX Secret Store without generating a key, reports missing-key status before the first encrypted drill, reports Secret-Store presence and key fingerprint after a drill, and never prints or embeds the raw key. The real local CLI restore drill exposed and fixed the large SQLite/ZIP64 boundary: portable snapshot ZIP entries now use `large_file(true)` and the manifest records `zip64_large_file_enabled=true`. Operator docs reference this status report as the evidence object for an external escrow record. This closes the software-side escrow observability gap; actual external escrow confirmation remains an operator/release signoff gate. | `rustfmt --edition 2021 --check src/core/business_os/store.rs src/core/service/business_os.rs src/core/secrets.rs`; `cargo test --bin ctox backup_restore_drill --no-default-features --target-dir runtime/build/business-os-backup-check-target` - 3 passed; `cargo test --bin ctox mcp_policy_value_args_are_deduplicated --no-default-features --target-dir runtime/build/business-os-backup-check-target` - 1 passed; `cargo build --bin ctox --no-default-features --target-dir runtime/build/business-os-backup-check-target` - completed with existing unrelated workspace warnings; real-root `ctox business-os backup restore-drill` passed with `ok=true`, signed manifest `04d0d523e18d9d32a203903c69cf29fa555099ded800a99b975c1730036fd8cf`, AES-256-GCM portable ciphertext `0f959fe413b1d59fae6b5891b05540da242a67bd5b19670f17b4c0737d8e39e9`, decrypted ZIP verification true and temp plaintext/verify ZIP deletion true; `ctox business-os backup key-escrow-status` returned `ready_for_external_escrow_confirmation` with `secret_value_revealed=false`; `ctox business-os backup inspect-manifest --manifest runtime/backup/business-os-drill-1781807557440-bec00472-78b8-4a62-ac02-737398289542/manifest.json` returned `ok=true`, valid HMAC, same-version compatibility allowed and ciphertext SHA-256 match; six-mode production Browser/Rust matrix artifact `runtime/build/business-os-production-smoke-key-escrow-zip64.json` passed all required modes with browser warnings/errors/404/request failures 0. |
