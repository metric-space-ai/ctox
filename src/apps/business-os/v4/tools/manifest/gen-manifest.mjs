#!/usr/bin/env node
// Shell v4 – Strang M/M2: erzeugt je App unter modules/ und customer-modules/ ein v4-Manifest und das Shell-Manifest
// (ctox-sync-v4-architektur.md §9.1, §10.2a, §16.13, §16.16, §16.17, §16.19–§16.22; shell-v4-entwurf.md §12 Nr. 1).
//
// Keine Abhängigkeiten außer Node-Bordmitteln. Deterministisch: keine Zeitstempel, alle Objektschlüssel
// sortiert, alle Listen sortiert. Felder, die sich nicht ableiten lassen, stehen als null mit `_todo`.
// Felder mit führendem Unterstrich sind Anmerkungen des Entwurfs (Herkunft, Vorschläge, offene Punkte)
// und keine Vertragsfelder.
//
// Quellen je App: module.json, collections.schema.json (einzige Schemaquelle, §16.13; eine korrigierte Fassung unter
// v4/schemas/<app_id>.collections.schema.json hat Vorrang, §16.21), handlers/*.js (Handler, Capabilities, Legacy,
// Befehlsrechte, commands_out, Trigger-Exporte, net.ingest, Konfigurationsschlüssel aus dem Code), UI-Code (commands_out
// über shared/context-chat.js bzw. payload.module), skills/ und prompts/ (§16.30) und je App eine Deklarationsdatei
// v4-manifest.json für das Nicht-Ableitbare (commands_in, exports, Rechte, multi_instance, Fremdzugriffs-ops, Trigger
// ohne Code-Export; Strang MAN). V4-HANDLERS.md/V4-PORT.md sind keine Quelle mehr (Wunschliste: WUENSCHE.md).
// schema.js ist optional und dient nur noch dem Drift-Bericht.
// Eigentum: OWNER_RESOLUTIONS (Plattform, OLG) vor ownership-baseline.json (eingefroren) vor Definierer/Präfix.
// Plattform-Collections gehören 'ctox-platform'; Apps deklarieren sie nur gemäß deren exports
// (~/.local/state/ctox-sync-v4/docs/v4-manifests/ctox-platform.manifest.json).
//
// Aufruf:
//   node gen-manifest.mjs                 schreibt ../../manifests/<app_id>.manifest.json (+ lib-<name>), out/* und REPORT-M2.md
//                                         und kopiert die gewählten Raster-Icons ins App-Paket (assets/icon.<ext>, MAN2)
//   node gen-manifest.mjs --check         vergleicht nur, Exit 1 bei Abweichung
//   node gen-manifest.mjs --shell-libs    nur shell.manifest.json + lib-<name>.manifest.json (§16.37), mit --check vergleichend
//   Optionen: --repo <root> --out <dir> --report-out <dir> --analysis <dir> --base-release <main@rev>
//             --platform-manifest <datei>

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import vm from 'node:vm';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';

import { isExcludedRuntimePath } from '../../../scripts/build-shell-artifact.mjs';
const HERE = path.dirname(fileURLToPath(import.meta.url));
// K-WJ-D1: V2-Mobilkatalog (Pin e35a116ae, mobile-apps.json) als Datenquelle für layout.presentation.mobile. Die Shell
// liest die Altdatei zur Laufzeit nicht mehr (Wächter ref-legacy-catalog-file); der Rahmen baut den Katalog aus kernel.apps.
const MOBILE_APPS = new Map(JSON.parse(fs.readFileSync(path.join(HERE, 'extra', 'mobile-apps.v2.json'), 'utf8')).apps
  .map(({ id, ...rest }) => [id, rest]));
export const DEFAULT_REPO = path.resolve(HERE, '../../../../../..');
export const DEFAULT_OUT = path.resolve(HERE, '../../manifests');
export const DEFAULT_REPORT_OUT = path.resolve(HERE, 'out');
export const DEFAULT_TOOL_OUT = HERE;
export const DEFAULT_ANALYSIS = path.join(os.homedir(), '.local/state/shell-v4/analysis');
export const DEFAULT_PLATFORM_MANIFEST = path.join(os.homedir(), '.local/state/ctox-sync-v4/docs/v4-manifests/ctox-platform.manifest.json');
export const DEFAULT_BASELINE = path.join(HERE, 'ownership-baseline.json');
export const BASE_RELEASE = 'main@4794af03d';
export const MIN_APP_API = '4.0';
export const MANIFEST_FORMAT = 'ctox-shell-v4-app-manifest-draft-1';
export const GENERATOR_ID = 'src/apps/business-os/v4/tools/manifest/gen-manifest.mjs';
export const APP_ROOTS = ['modules', 'customer-modules'];

// Kernel/Core-Plattformmanifest (§16.17). Die Shell ist eine eigene App 'shell' ohne eigene Collections.
export const PLATFORM_OWNER = 'ctox-platform';
export const SHELL_APP_ID = 'shell';

// Plattform-Collections (§16.17, §16.19 Nr. 3, §16.22). Apps deklarieren sie nur als foreign gemäß Plattform-exports.
export const PLATFORM_COLLECTIONS = [
  'app_activations', 'app_drafts', 'business_app_versions', 'business_apps', 'business_commands',
  'business_module_catalog', 'ctox_queue_tasks', 'ctox_runtime_settings', 'files', 'shell_preferences',
];
// Rückfall, falls das Plattformmanifest nicht lesbar ist (Stand ctox-platform.manifest.json 15.10. 13:11).
export const PLATFORM_FALLBACK_EXPORTS = {
  app_activations: ['read', 'write'],
  business_app_versions: ['read'],
  business_apps: ['read'],
  business_commands: ['read', 'write'],
  ctox_queue_tasks: ['read'],
  ctox_runtime_settings: ['read'],
  files: ['read', 'write'],
  shell_preferences: ['read', 'write'],
};
// business_module_catalog entfällt (§16.14/§16.17): Zugriff stattdessen auf business_apps/business_app_versions.
export const PLATFORM_REPLACEMENTS = { business_module_catalog: ['business_apps', 'business_app_versions'] };

// Eigentümer-Entscheidungen mit Vorrang vor der Basis-Zuordnung.
const platformBasis = (detail) => ({ owner: PLATFORM_OWNER, basis: `Plattform-Collection (${detail})` });
export const OWNER_RESOLUTIONS = {
  app_activations: platformBasis('§16.17, ctox-platform.manifest.json'),
  app_drafts: platformBasis('§16.22: Entwürfe je (App, Nutzer), Eigentum ctox-platform'),
  business_app_versions: platformBasis('§9.2, §16.17'),
  business_apps: platformBasis('§9.2, §16.14, §16.17'),
  business_commands: platformBasis('§16.17; Shell-registriert app.js:211-216'),
  business_module_catalog: platformBasis('§16.17: entfällt, ersetzt durch business_apps/business_app_versions'),
  ctox_queue_tasks: platformBasis('§16.17; CTOX-Queue gehört dem Core'),
  ctox_runtime_settings: platformBasis('§16.17'),
  files: platformBasis('§10.2a api.files, ctox-platform.manifest.json'),
  shell_preferences: platformBasis('§16.19 Nr. 3'),
  outbound_lead_generation_adapters: { owner: 'outbound-lead-generation', basis: '§16.21 Eigentum (Registry, Basis-Release-Import): OLG; browser deklariert foreign' },
  outbound_research_policies: { owner: 'outbound-lead-generation', basis: '§16.21 Eigentum: OLG (einziger Schreiber outbound.research_policy.publish; heute nur nativ in business_records)' },
  sellify_campaigns: { owner: 'outbound-lead-generation', basis: '§16.21 Eigentum: OLG (Projektion der externen SQL-Quelle Sellify)' },
  sellify_companies: { owner: 'outbound-lead-generation', basis: '§16.21 Eigentum: OLG (Projektion der externen SQL-Quelle Sellify)' },
  sellify_people: { owner: 'outbound-lead-generation', basis: '§16.21 Eigentum: OLG (Projektion der externen SQL-Quelle Sellify)' },
};

// Präfix-Regeln: nur für Collections, die weder entschieden noch in der Basis-Zuordnung stehen.
export const PREFIX_OWNERS = [
  ['accounting_invoice', 'invoices'],
  ['accounting_credit_notes', 'invoices'],
  ['accounting_payment', 'invoices'],
  ['accounting_dunning', 'invoices'],
  ['accounting_recurring', 'invoices'],
  ['accounting_', 'buchhaltung'],
  ['appsec_', 'appsec-pentest'],
  ['browser_', 'browser'],
  ['calendar_', 'calendar'],
  ['coding_agent_', 'coding-agents'],
  ['communication_', 'conversations'],
  ['ctox_ticket_', 'tickets'],
  ['ctox_', 'ctox'],
  ['customer_', 'customers'],
  ['desktop_', 'desktop'],
  ['document_', 'documents'],
  ['interview_', 'interviews'],
  ['iot_', 'iot'],
  ['knowledge_', 'knowledge'],
  ['kundenpipeline_', 'kundenpipeline'],
  ['matching_', 'matching'],
  ['news_', 'news'],
  ['outbound_lead_generation_', 'outbound-lead-generation'],
  ['outbound_', 'outbound'],
  ['planning_', 'shiftflow'],
  ['research_', 'research'],
  ['sellify_', 'outbound-lead-generation'],
  ['spreadsheet', 'spreadsheets'],
  ['support_', 'support'],
  ['user_thread', 'threads'],
  ['user_notifications', 'threads'],
  ['workjet_', 'ctox'],
  ['business_chats', 'ctox'],
  ['business_consents', 'consent'],
  ['business_credentials', 'nachweise'],
  ['signature_requests', 'esign'],
  ['applications', 'intake'],
  ['placements', 'placements'],
  ['submissions', 'submissions'],
  ['offers', 'placements'],
  ['notes', 'notes'],
  ['documents', 'documents'],
];

// Klassen-Belege außerhalb von sync.js (Ist-Aufnahme). Nur wo ein Beleg existiert.
export const PROJECTION_EVIDENCE = {
  business_module_catalog: 'C1 §1 app-store: "Katalogprojektion" (store_catalog_projections.rs); D-native §2',
  ctox_harness_status: 'C1 §11 ctox: Harness-Projektion aus dem Core (rxdb_peer_projections.rs)',
  sellify_campaigns: 'Projektion der externen SQL-Quelle (external_sql_sync.rs); heute server demand-only (rxdb_peer.rs:1036-1046)',
  sellify_companies: 'Projektion der externen SQL-Quelle (external_sql_sync.rs); heute server demand-only (rxdb_peer.rs:1036-1046)',
  sellify_people: 'Projektion der externen SQL-Quelle (external_sql_sync.rs); heute server demand-only (rxdb_peer.rs:1036-1046)',
};

// Deklarationsdatei je App (Strang MAN, 15.10.2026): modules/<app>/v4-manifest.json bzw. customer-modules/<app>/v4-manifest.json.
// Sie trägt NUR, was sich nicht aus dem Code ableiten lässt (commands_in, zusätzliche exports, Rechte, multi_instance,
// Fremdzugriffs-ops, die der Zeilen-Scan nicht belegen kann, Trigger ohne Code-Export, Klassen-/Filter-Ergänzungen).
// Keine freien Texte: V4-HANDLERS.md/V4-PORT.md sind keine Quelle des Generators mehr. `_basis` nennt je Feld die
// Fundstelle (Anmerkung, wird nicht ausgewertet). Unbekannte Schlüssel, falsche ops oder Trigger-Formen sind Verstöße
// (declaration_check, Exit 1).
export const DECLARATION_FILE = 'v4-manifest.json';
export const DECLARATION_FORMAT = 'ctox-shell-v4-app-declaration-1';
export const DECLARATION_KEYS = [
  '_basis', '_format', '_note', '_proposed_triggers',
  'class_proposals', 'collection_overrides', 'collection_rights', 'command_rights', 'commands_in', 'commands_out',
  'exports', 'foreign_collections', 'foreign_drop', 'foreign_exclusive', 'layout_icon_svg_from', 'multi_instance',
  'row_filters', 'styles', 'todo', 'triggers', 'write_guard',
];
const VALID_OPS = ['read', 'write', 'watch'];

// Dynamische Capability-Pfade, die sich nicht als Literal finden (Fundstelle im Handler-Code). Leer seit iot IOT4:
// kein Handler importiert mehr ./_iot.js (modules/iot/V4-HANDLERS.md „Trigger“).
export const DYNAMIC_CAPS = [];

// Shell-Manifest: Dateien laut Dateigrenze (~/.local/state/ctox-sync-v4/docs/dateigrenzen.md, Abschnitt „Shell v4“).
export const SHELL_TREES = ['v4/frame', 'v4/shell', 'v4/source'];
export const SHELL_SINGLE_FILES = ['v4/sw.js'];
export const SHELL_SHARED_PATTERNS = [
  'app.js', 'app.css', 'index.html', 'mobile-host.*', 'app-actions.js', 'app-lifecycle.js', 'base.css',
  'branding.js', 'business-chat.js', 'business-companions.js', 'business-reporter.js', 'context-menu.js',
  'business-permissions.js', 'crew-motion.js', 'crew-presence.js', 'crew-renderer.js', 'dialogs.js', 'documents.js', 'event-bus.js', 'grok-shell-icon-selection.js',
  'i18n.js', 'icons.js', 'launcher-icon.js', 'model-access-health.js', 'notifications.js',
  'office-workspace.css', 'operator-icon-selection.js', 'pane-grammar.js', 'permissions.js', 'roles.js', 'presentation.js',
  'react-settings.js', 'resizer.js', 'sanitize-rich-html.js', 'shell-chat-composition.js', 'shell-permissions-ui.js',
  'shell-performance-trace.js', 'shell-topbar.css', 'stable-dom.js', 'taskbar-pins.js', 'topbar-apps.js',
  'universal-importer.*', 'window-*.js', 'workjet-theme.js', 'workspace-session.js',
  // ctox:shared/prompt-template.js – Template-Helfer für Handler (§16.30 Nr. 4, Strang SKL0).
  'prompt-template.js',
];

// Fremdzugriffe der Shell (§10.2a Nr. 8: die Shell ist App 'shell' ohne eigene Collections). Eigentümer laut
// Eigentumszuordnung; generate() prüft sie und trägt die ops in die exports der Eigentümer-App ein.
export const SHELL_FOREIGN = {
  shell_preferences: { owner_app: PLATFORM_OWNER, ops: ['read', 'write'], sources: ['§16.19 Nr. 3 (Pins, Launcher-Reihenfolge über api.put)'] },
  // MAN2: Meldeschleife FR6b (v4/shell/report.js, REPORT_COLLECTIONS) speichert Meldungen wie „Bugs & Features“.
  business_module_reports: { owner_app: 'ctox', ops: ['read', 'write'], sources: ['v4/shell/report.js REPORT_COLLECTIONS.reports (FR6b Meldeschleife, api.put)'] },
  ctox_bug_reports: { owner_app: 'ctox', ops: ['read', 'write'], sources: ['v4/shell/report.js REPORT_COLLECTIONS.bugs (FR6b Meldeschleife, api.put)'] },
  // LOOP Runde 2: Melder-Name im Reiter „Meldungen“ aus der Nutzerliste der Instanz (nur lesen).
  business_users: { owner_app: 'ctox', ops: ['read'], sources: ['v4/source/reports-tab.js USERS_COLLECTION (Melder-Name, Rückfall „Teammitglied“)'] },
  // E30/K-WJ-1: die Workjet-Brücke liest als App 'shell'. Ohne Deklaration verweigert der Kern das Lesen (files.get → forbidden).
  workjet_projects: { owner_app: 'ctox', ops: ['read'], sources: ['v4/shell/workjet/actions/project.js (project.list, startCollection workjet_projects)'] },
  workjet_working_copies: { owner_app: 'ctox', ops: ['read'], sources: ['v4/shell/workjet/actions/project.js (project.list, Arbeitskopien)'] },
  workjet_computers: { owner_app: 'ctox', ops: ['read'], sources: ['v4/shell/workjet/actions/computer.js (computer.list, E32 lokales Lesen)'] },
  workjet_sessions: { owner_app: 'ctox', ops: ['read'], sources: ['v4/shell/workjet/actions/session.js (session.list, E32 lokales Lesen)'] },
  workjet_session_transfers: { owner_app: 'ctox', ops: ['read'], sources: ['v4/shell/workjet/actions/session.js (Sitzungsübergabe)'] },
  desktop_files: { owner_app: 'desktop', ops: ['read'], sources: ['v4/shell/workjet/actions/project.js presentation.content.read, actions/meeting.js narration.read (files.get über desktop_files-Zeile, E30)'] },
  // CHAT4: die V2-Chatleiste (shared/business-chat.js über v4/shell/chat, App 'shell') liest und schreibt die eigenen Verläufe
  // und liest Befehls-, Queue- und Crew-Zustand (Inventar v4/shell/chat/CHAT4.md).
  business_chats: { owner_app: 'ctox', ops: ['read', 'write'], sources: ['v4/shell/chat/index.js CHAT_WRITABLE (shared/business-chat.js hydrateChatsFromRxDb, persistChatDocsRemote, deleteChat)'] },
  business_commands: { owner_app: PLATFORM_OWNER, ops: ['read'], sources: ['v4/shell/chat/index.js CHAT_COLLECTIONS (shared/business-chat.js waitForSubmittedTaskId, syncTrackedMessages)'] },
  ctox_queue_tasks: { owner_app: PLATFORM_OWNER, ops: ['read'], sources: ['v4/shell/chat/index.js CHAT_COLLECTIONS (shared/business-chat.js syncTrackedMessages, loadCrewAppTasks)'] },
  ctox_crew_members: { owner_app: 'ctox', ops: ['read'], sources: ['v4/shell/chat/index.js CHAT_COLLECTIONS (shared/business-chat.js loadCrewMembers, Crew-Pool der Leiste)'] },
  ctox_harness_status: { owner_app: 'ctox', ops: ['read'], sources: ['v4/shell/chat/index.js CHAT_COLLECTIONS (shared/business-chat.js loadCrewHarnessStatus, Arbeiterwahrheit)'] },
  ctox_harness_events: { owner_app: 'ctox', ops: ['read'], sources: ['v4/shell/chat/index.js CHAT_COLLECTIONS (shared/business-chat.js crewSelectionReason)'] },
};

// ROUTE (Kernentscheidung E39, Präzisierung §16.40): Befehle der Shell an fremde Apps. Der Chat der Shell
// (v4/shell/chat → shared/business-chat.js, commandBus.dispatch) sendet business_os.chat.task – E46: Plattformroute,
// nativer Kern-Chatpfad, keine Deklaration –, „CTOX fragen…“
// (v4/shell/app-host.js Menüpunkt ctox.ask → Eingabeblatt) den Standardbefehl ctox.chat.task. Empfänger ist ctox.
export const SHELL_COMMANDS_OUT = [
  { app: 'ctox', type: 'ctox.chat.task', sources: ['v4/shell/app-host.js:500 (Menüpunkt ctox.ask „CTOX fragen…“ → Eingabeblatt → Standardbefehl ctox.chat.task)'] },
];

// Kernentscheidung E40/E42: Plattformrouten brauchen weder commands_out noch Handler. Maßgeblich ist die Inventur des
// Kerns (exact_control_types, predicate_control_types, control_prefixes, browser_runtime_types, browser_literal_types)
// plus Chat-Befehle und die Registry-Routen aus E25/E27.6 (PLATFORM_COMMANDS in kernel/fake-core-runtime.mjs).
export const COMMAND_INVENTORY_FILE = 'src/core/business_os/business_command_inventory.json';
// E42: Chat-Befehle führt der Kern über den Chat-Pfad aus (store_projections.rs is_business_chat_command).
export const PLATFORM_CHAT_ROUTES = ['business_os.chat.task', 'business_os.context.ask', 'business_os.data.modify'];
export const PLATFORM_REGISTRY_ROUTES = [
  'ctox.app.publish', 'ctox.app.release', 'ctox.app.remove', 'ctox.app.rollback', 'ctox.app.update',
  'ctox.coding.cancel', 'ctox.coding.turn',
];
// E40/E42: Registry-Routen, die noch nicht auf v4/integration liegen (E25 von Transfer); bis dahin nicht rot. Jeder Eintrag erscheint im Bericht; ein Typ außerhalb dieser Tabelle ohne Route bleibt rot.
export const PLATFORM_ROUTES_PENDING = {
  'browser.http_auth.respond': 'V2-gleich ohne Handler, native Browser-Laufzeit-Handler folgen nach M7 (E44.1, CAP)',
  'browser.observer.grant': 'V2-gleich ohne Handler, native Browser-Laufzeit-Handler folgen nach M7 (E44.1, CAP)',
  'browser.observer.revoke': 'V2-gleich ohne Handler, native Browser-Laufzeit-Handler folgen nach M7 (E44.1, CAP)',
  'browser.upload.select': 'V2-gleich ohne Handler, native Browser-Laufzeit-Handler folgen nach M7 (E44.1, CAP)',
  'browser.webauthn.respond': 'V2-gleich ohne Handler, native Browser-Laufzeit-Handler folgen nach M7 (E44.1, CAP)',
  'ctox.plan.step.retry': 'E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen',
  'ctox.workjet.calendar.connect.v1': 'E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen',
  'ctox.workjet.calendar.read.v1': 'E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen',
  'ctox.workjet.device.v1': 'E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen',
  'ctox.workjet.grok.v1': 'E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen',
  'ctox.workjet.jour_fixe.speech.v1': 'E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen',
  'ctox.workjet.speech.dictation.v1': 'E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen',
  'ctox.workjet.speech.settings.v1': 'E42: Registry-Route, sobald die E25-Routen von Transfer auf v4/integration liegen',
};

export function loadPlatformRoutes(repo) {
  const file = path.join(repo, COMMAND_INVENTORY_FILE);
  const inv = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : {};
  return {
    source: fs.existsSync(file) ? COMMAND_INVENTORY_FILE : null,
    // E42: alle Klassen der Inventur (Literal-Typen ohne nativen Handler führt der Kern als CTOX-Queue-Task aus).
    exact: new Set([
      ...(inv.exact_control_types || []), ...(inv.predicate_control_types || []),
      ...(inv.browser_runtime_types || []), ...(inv.browser_literal_types || []),
      ...PLATFORM_CHAT_ROUTES, ...PLATFORM_REGISTRY_ROUTES,
    ]),
    prefixes: [...(inv.control_prefixes || [])],
  };
}

// Umschließendes Objektliteral von pos (Klammerzählung, Strings grob übersprungen) auf `dispatch: false` prüfen.
export function enclosingObjectHasDispatchFalse(text, pos) {
  let depth = 0, start = -1;
  for (let i = pos; i >= 0; i -= 1) {
    const c = text[i];
    if (c === '}') depth += 1;
    else if (c === '{') { if (depth === 0) { start = i; break; } depth -= 1; }
  }
  if (start < 0) return false;
  depth = 0;
  for (let i = start; i < text.length; i += 1) {
    const c = text[i];
    if (c === '{') depth += 1;
    else if (c === '}') { depth -= 1; if (depth === 0) return /\bdispatch\s*:\s*false\b/.test(text.slice(start, i + 1)); }
  }
  return false;
}

export function isPlatformRoute(type, routes) {
  return routes.exact.has(type) || routes.prefixes.some((p) => type.startsWith(p));
}

// AGENT (§16.44): die Code-Ansicht der Shell sendet ctox.coding.turn / ctox.coding.cancel (v4/source/agent-panel.js).
// OS-Katalog: Präfix agent.coding. unter agent.turn (v4/os-primitives/catalog.mjs).
export const SHELL_CAPABILITIES = {
  'agent.coding.cancel': ['v4/source/agent-panel.js (ctox.coding.cancel, Abbrechen in der Agent-Seitenleiste)'],
  'agent.coding.turn': ['v4/source/agent-panel.js (ctox.coding.turn, Auftrag aus der Agent-Seitenleiste)'],
};

// MAN2 (M-09): Raster-Icons. EINE Quelle sind die Auswahlprojektionen, die Shell-V2 liest (app.js
// getRegisteredSvgIcon: operatorIconFor vor grokShellIconFor vor module.json layout.icon_asset). Apps stehen für sich:
// das gewählte Bild liegt als Kopie im App-Paket (ICON_PACKAGE_BASENAME + Endung), außer es liegt schon im eigenen
// Paket (knowledge). Der Schreiblauf kopiert, --check meldet fehlende/abweichende Kopien (icon_package, Exit 1).
export const ICON_SELECTION_SOURCES = [
  { file: 'shared/operator-icon-selection.js', table: 'OPERATOR_ICON_SELECTION' },
  { file: 'shared/grok-shell-icon-selection.js', table: 'GROK_SHELL_ICON_SELECTION' },
];
export const ICON_PACKAGE_BASENAME = 'assets/icon';

// Relative Importe (statisch `from '…'`, `import '…'`, dynamisch `import('…')`, `new URL('…', import.meta.url)`) der Shell-Dateien, transitiv, innerhalb
// von src/apps/business-os. Cache-Buster (?v=) wird abgeschnitten; nur vorhandene .js/.mjs/.css/.json-Dateien zählen.
export function shellImportClosure(appsRoot, seeds) {
  const seen = new Set(seeds);
  const out = [];
  const queue = seeds.filter((r) => /\.(m?js)$/.test(r));
  // BOOTR OPEN-4: auch new URL('./x.css', import.meta.url) – so lädt der Rahmen sein CSS (frame/index.js cssLinks),
  // ebenso Code-Ansicht und Chat. Ohne diese Dateien im Manifest liefert der SW 404 und der Rahmen steht ungestylt.
  const re = /(?:\bfrom\s*|\bimport\s*\(\s*|\bimport\s+|\bnew\s+URL\s*\(\s*)(['"])(\.{1,2}\/[^'"?#]+)(?:[?#][^'"]*)?\1/g;
  while (queue.length) {
    const rel = queue.shift();
    let text;
    try { text = fs.readFileSync(path.join(appsRoot, rel), 'utf8'); } catch { continue; }
    for (const m of text.matchAll(re)) {
      const target = path.posix.normalize(path.posix.join(path.posix.dirname(rel), m[2]));
      if (target.startsWith('..') || seen.has(target) || !/\.(m?js|css|json)$/.test(target)) continue;
      if (!fs.existsSync(path.join(appsRoot, target)) || !fs.statSync(path.join(appsRoot, target)).isFile()) continue;
      seen.add(target);
      out.push(target);
      if (/\.(m?js)$/.test(target)) queue.push(target);
    }
  }
  return out.sort();
}

// §16.36 Nr. 3: Module, die Handler im Daemon als ctox:shared/<pfad> importieren dürfen – nur reiner Code
// (kein DOM, kein Netz, kein Speicher). Alles andere ergibt im AppRuntimeService einen Importfehler.
export const SHELL_SHARED_EXPORTS = ['shared/prompt-template.js'];

// §16.37: Standardbibliotheken sind eigene Plattform-Artefakte lib:<name> (major.minor). Freigabeliste und Quellordner:
// v4/tools/isolation/standard-libs.json (nur status "approved"); das Shell-Manifest bindet libs: {<name>: "<major>"}.
export const STANDARD_LIBS_FILE = 'v4/tools/isolation/standard-libs.json';
export const LIB_MANIFEST_FORMAT = 'ctox-shell-v4-lib-manifest-draft-1';
// Woher die Paketversion kommt, wenn standard-libs.json keine "version" nennt (erster Treffer gilt).
export const LIB_VERSION_PROBES = ['package.json', 'vs/loader.js'];

const WRITE_METHOD_RE = /\.(insert|upsert|bulkInsert|bulkUpsert|incrementalUpsert|incrementalPatch|patch|incrementalModify|modify|remove|bulkRemove|put)\(/;
const CODE_EXT_RE = /\.(m?js)$/;

// ---------------------------------------------------------------- Hilfsfunktionen

export function canon(value) {
  if (Array.isArray(value)) return value.map(canon);
  if (value && typeof value === 'object') {
    const out = {};
    for (const key of Object.keys(value).sort()) {
      if (value[key] === undefined) continue;
      out[key] = canon(value[key]);
    }
    return out;
  }
  return value;
}

export function stableStringify(value) {
  return `${JSON.stringify(canon(value), null, 2)}\n`;
}

function sha256(buf) {
  return crypto.createHash('sha256').update(buf).digest('hex');
}

// Handler-Dateien (handlers/<command_type>.js, §16.16) sind nie Testdateien, auch wenn der Befehlstyp „test“ enthält
// (z. B. outbound.research_source.test).
export function isHandlerFile(rel) {
  return /^handlers\/[^/_][^/]*\.js$/.test(rel);
}

export function isTestPath(rel) {
  if (isHandlerFile(rel)) return false;
  const parts = rel.split('/');
  if (parts.slice(0, -1).some((p) => p === 'tests' || p === 'test' || p === '__tests__')) return true;
  return /test|smoke/i.test(parts[parts.length - 1]);
}

function walk(dir, base = dir, acc = []) {
  if (!fs.existsSync(dir)) return acc;
  for (const name of fs.readdirSync(dir).sort()) {
    if (name === '.DS_Store' || name === 'node_modules') continue;
    const abs = path.join(dir, name);
    const st = fs.lstatSync(abs);
    if (st.isSymbolicLink()) continue;
    if (st.isDirectory()) walk(abs, base, acc);
    else if (st.isFile()) acc.push(path.relative(base, abs).split(path.sep).join('/'));
  }
  return acc;
}

export function hashFiles(appDir) {
  const files = {};
  for (const rel of walk(appDir)) {
    if (isTestPath(rel)) continue;
    files[rel] = sha256(fs.readFileSync(path.join(appDir, rel)));
  }
  return files;
}

export function baseHash(files) {
  const lines = Object.keys(files).sort().map((p) => `${p}\t${files[p]}\n`).join('');
  return sha256(Buffer.from(lines, 'utf8'));
}

function readJson(p) {
  return JSON.parse(fs.readFileSync(p, 'utf8'));
}

function lineOf(text, index) {
  return text.slice(0, index).split('\n').length;
}

function sortedUnique(list) {
  return [...new Set(list)].sort();
}

const OP_ORDER = ['read', 'write', 'watch'];
function sortOps(list) {
  return [...new Set(list)].sort((a, b) => OP_ORDER.indexOf(a) - OP_ORDER.indexOf(b));
}

// Zeilen-Kommentare und Blockkommentare am Zeilenanfang entfernen (Handler-Code; Kommentare nennen oft Capabilities).
export function stripLeadingComments(text) {
  const out = [];
  let inBlock = false;
  for (const line of text.split('\n')) {
    const t = line.trim();
    if (inBlock) {
      if (t.includes('*/')) inBlock = false;
      out.push('');
      continue;
    }
    if (t.startsWith('//')) { out.push(''); continue; }
    if (t.startsWith('/*')) {
      if (!t.includes('*/')) inBlock = true;
      out.push('');
      continue;
    }
    if (t.startsWith('*')) { out.push(''); continue; }
    out.push(line);
  }
  return out.join('\n');
}

// ---------------------------------------------------------------- Quellen laden

// collections.schema.json in die Form {collections:{name:Schema}, wrappers, migrations, migrationFormat} bringen.
// Akzeptiert A9 (`migrations: {name: {from_ver, steps|migrate_fn}}`) und das Altformat (`migration_strategies:
// {name: {ver: strategie}}`); Einträge der Form {schema, syncProfile …} werden ausgepackt.
export function normalizeCollectionsSchema(raw) {
  const collections = {};
  const wrappers = {};
  for (const [name, def] of Object.entries(raw?.collections || {})) {
    const isWrapper = def && typeof def === 'object' && def.schema && typeof def.schema === 'object' && !def.properties;
    collections[name] = isWrapper ? def.schema : def;
    if (isWrapper) {
      const extra = { ...def };
      delete extra.schema;
      wrappers[name] = extra;
    }
  }
  const migrations = {};
  let migrationFormat = null;
  if (raw?.migrations && typeof raw.migrations === 'object') {
    migrationFormat = 'migrations';
    for (const [name, m] of Object.entries(raw.migrations)) migrations[name] = { kind: 'a9', value: m };
  }
  if (raw?.migration_strategies && typeof raw.migration_strategies === 'object') {
    migrationFormat ||= 'migration_strategies';
    for (const [name, s] of Object.entries(raw.migration_strategies)) {
      if (!migrations[name] && s && Object.keys(s).length) migrations[name] = { kind: 'strategies', value: s };
    }
  }
  return { collections, wrappers, migrations, migrationFormat };
}

export function migrationSteps(entry) {
  if (!entry) return [];
  if (entry.kind === 'strategies') return Object.keys(entry.value).map(Number).filter(Number.isFinite).sort((a, b) => a - b);
  const steps = Array.isArray(entry.value?.steps) ? entry.value.steps : [];
  return steps.map((s) => Number(s?.to_ver)).filter(Number.isFinite).sort((a, b) => a - b);
}

async function loadSchemaJs(appDir, repoAppsDir) {
  const file = path.join(appDir, 'schema.js');
  if (!fs.existsSync(file)) return { value: null, error: null };
  const text = fs.readFileSync(file, 'utf8');
  const imports = [];
  const re = /from\s+['"]([^'"]*schema\.js)(\?[^'"]*)?['"]/g;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    const target = path.resolve(appDir, m[1]);
    const rel = path.basename(path.dirname(target));
    if (!path.relative(repoAppsDir, target).startsWith('..')) imports.push(rel);
  }
  let mod;
  try {
    mod = await import(pathToFileURL(file).href);
  } catch (err) {
    return { value: null, error: `${err.code || 'Fehler'}: ${String(err.message).split('\n')[0].split(repoAppsDir).join('…')}` };
  }
  const collections = {};
  for (const [name, def] of Object.entries(mod.collections || {})) {
    const isWrapper = def && typeof def === 'object' && def.schema && typeof def.schema === 'object';
    collections[name] = JSON.parse(JSON.stringify(isWrapper ? def.schema : def));
  }
  const steps = {};
  for (const [name, s] of Object.entries(mod.migrationStrategies || {})) {
    steps[name] = Object.keys(s || {}).map(Number).filter(Number.isFinite).sort((a, b) => a - b);
  }
  return { value: { imports: sortedUnique(imports), collections, migrationSteps: steps }, error: null };
}

function loadDemandOnly(repo) {
  const file = path.join(repo, 'src/apps/business-os/shared/sync.js');
  if (!fs.existsSync(file)) return {};
  const text = fs.readFileSync(file, 'utf8');
  const start = text.indexOf('function isDemandOnlyPullCollection');
  if (start < 0) return {};
  const end = text.indexOf('return true;', start);
  const block = text.slice(start, end);
  const out = {};
  const re = /collection === '([a-z0-9_]+)'/g;
  for (let m = re.exec(block); m; m = re.exec(block)) {
    out[m[1]] = `shared/sync.js:${lineOf(text, start + m.index)} isDemandOnlyPullCollection`;
  }
  return out;
}

function loadAnalysisSections(analysisDir) {
  const sections = {};
  for (const doc of ['C1-apps-teil1.md', 'C2-apps-teil2.md']) {
    const file = path.join(analysisDir, doc);
    if (!fs.existsSync(file)) continue;
    const text = fs.readFileSync(file, 'utf8');
    for (const part of text.split(/^## /m).slice(1)) {
      const heading = part.split('\n')[0];
      const m = /^(?:\d+\.\s+|modules\/)([a-z0-9-]+)/.exec(heading);
      if (!m) continue;
      const body = part.slice(heading.length);
      const r = /^\s*\d+\.\s+\*\*Rust\*\*[^\n]*(?:\n(?!\s*\d+\.\s+\*\*)[^\n]*)*/m.exec(body);
      const rust = r ? r[0].trim().replace(/^\s*\d+\.\s+\*\*Rust\*\*:?\s*/, '').replace(/\s+/g, ' ').slice(0, 240) : '';
      sections[m[1]] = { rust };
    }
  }
  return sections;
}

export function loadPlatform(file) {
  if (file && fs.existsSync(file)) {
    const m = readJson(file);
    const exports = {};
    for (const [name, ops] of Object.entries(m.exports || {})) if (!name.startsWith('_')) exports[name] = sortOps(ops);
    const collections = Object.keys(m.collections || {}).filter((n) => !n.startsWith('_'));
    return { exports, collections, source: file.startsWith(os.homedir()) ? `~${file.slice(os.homedir().length)}` : file };
  }
  return { exports: { ...PLATFORM_FALLBACK_EXPORTS }, collections: Object.keys(PLATFORM_FALLBACK_EXPORTS), source: 'PLATFORM_FALLBACK_EXPORTS (gen-manifest.mjs)' };
}

// ---------------------------------------------------------------- Handler-Code

export function listHandlers(appDir) {
  const dir = path.join(appDir, 'handlers');
  if (!fs.existsSync(dir)) return {};
  const out = {};
  for (const name of fs.readdirSync(dir).sort()) {
    if (!name.endsWith('.js') || name.startsWith('_')) continue;
    if (!fs.statSync(path.join(dir, name)).isFile()) continue;
    out[name.slice(0, -3)] = `handlers/${name}`;
  }
  return out;
}

// Capabilities, Legacy-Typen und Befehlsrechte aus dem Handler-Code (handlers/**, ohne Tests).
export function scanHandlerCode(appDir, handlers) {
  const caps = new Map(); // name -> Set(file)
  const legacy = new Map();
  const rights = {};
  const addCap = (name, file) => {
    const n = name.replace(/\?\./g, '.').replace(/^\./, '');
    if (!n) return;
    if (n === 'legacy' || n.startsWith('legacy.')) return;
    if (!caps.has(n)) caps.set(n, new Set());
    caps.get(n).add(file);
  };
  const files = walk(path.join(appDir, 'handlers')).map((f) => `handlers/${f}`).filter((f) => CODE_EXT_RE.test(f) && !isTestPath(f));
  for (const rel of files) {
    const text = stripLeadingComments(fs.readFileSync(path.join(appDir, rel), 'utf8'));
    for (const m of text.matchAll(/ctx\??\.cap((?:\??\.[A-Za-z_]\w*)+)/g)) {
      if (/^\??\.legacy\b/.test(m[1])) continue;
      addCap(m[1], rel);
    }
    for (const m of text.matchAll(/(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*ctx\??\.cap((?:\??\.[A-Za-z_]\w*)+)/g)) {
      const base = m[2].replace(/\?\./g, '.');
      const callRe = new RegExp(`\\b${m[1].replace(/\$/g, '\\$')}(?:\\?\\.|\\.)([A-Za-z_]\\w*)\\s*\\(`, 'g');
      let found = false;
      for (const c of text.matchAll(callRe)) { addCap(`${base}.${c[1]}`, rel); found = true; }
      if (!found) addCap(base, rel);
    }
    for (const m of text.matchAll(/\b(?:callCap|capability)\(\s*ctx\s*,\s*['"]([\w.]+)['"]/g)) addCap(m[1], rel);
    // CAP3B/MAN3: ctx.model(req) ist die Modell-Bindung der App-Laufzeit (#830), kein ctx.cap-Pfad; der Kern gibt sie nur
    // mit dem Grant `model` in capabilities_required frei (capability_available("model")).
    if (/\bctx\??\.model\??\.?\(/.test(text)) addCap('model', rel);
    for (const m of text.matchAll(/\bcap\(\s*ctx\s*,\s*\[([^\]]*)\]/g)) {
      const parts = [...m[1].matchAll(/['"](\w+)['"]/g)].map((x) => x[1]);
      if (parts.length) addCap(parts.join('.'), rel);
    }
    for (const m of text.matchAll(/cap\??\.legacy\s*\[\s*['"]([^'"]+)['"]\s*\]/g)) {
      if (!legacy.has(m[1])) legacy.set(m[1], new Set());
      legacy.get(m[1]).add(rel);
    }
    for (const m of text.matchAll(/\blegacyForwarder\(\s*['"]([^'"]+)['"]\s*\)/g)) {
      if (!legacy.has(m[1])) legacy.set(m[1], new Set());
      legacy.get(m[1]).add(rel);
    }
    const perm = /export\s+const\s+([A-Z_]+_COMMAND_PERMISSIONS)\s*=\s*Object\.freeze\(\{([\s\S]*?)\}\)/.exec(text);
    if (perm) {
      for (const p of perm[2].matchAll(/['"]([\w.*]+)['"]\s*:\s*['"]([\w.]+)['"]/g)) {
        rights[p[1]] = { roles: ['admin', 'chef'], assigned_users: true, _permission: p[2], _basis: `${rel} ${perm[1]}; Core prüft Chef/Admin oder Modulzuweisung (policy.rs:483-490)` };
      }
    }
  }
  // Dynamische Pfade (DYNAMIC_CAPS): Handler, die den Marker importieren, rufen <capPrefix><op>.
  for (const rule of DYNAMIC_CAPS) {
    for (const [type, rel] of Object.entries(handlers)) {
      const text = fs.readFileSync(path.join(appDir, rel), 'utf8');
      if (!text.includes(rule.importMarker) || !type.startsWith(rule.typePrefix)) continue;
      addCap(`${rule.capPrefix}${type.slice(rule.typePrefix.length)}`, rel);
    }
  }
  // Präfixe weglassen, wenn ein längerer Pfad derselben App existiert (cap.ctox.ticket neben cap.ctox.ticket.run).
  const names = [...caps.keys()];
  const kept = names.filter((n) => !names.some((o) => o !== n && o.startsWith(`${n}.`))).sort();
  return {
    capabilities: Object.fromEntries(kept.map((n) => [n, [...caps.get(n)].sort()])),
    legacy: Object.fromEntries([...legacy.keys()].sort().map((t) => [t, [...legacy.get(t)].sort()])),
    rights,
  };
}

// ---------------------------------------------------------------- Skills und Prompt-Templates (§16.30, §16.34)

// Skill- und Template-Namen: Kleinbuchstaben, Ziffern, Bindestrich (Namensraum `<app_id>/<name>` im Harness).
export const AGENT_NAME_RE = /^[a-z0-9][a-z0-9-]*$/;
// Skill-Verweis in App-Code: global (`systematic-research`, System-Skill des Core) oder `<app_id>/<name>`.
export const SKILL_REF_RE = /^[a-z0-9][a-z0-9_-]*(?:\/[a-z0-9][a-z0-9_-]*)?$/;

// Paketdateien `skills/<name>/SKILL.md` (+ references/) und `prompts/<name>.md` → Manifestfelder skills/prompts.
// problems: Skill-Ordner ohne SKILL.md, ungültige Namen, Dateien, die die Testdatei-Regel aus files entfernen würde.
export function collectAgentPackage(appDir) {
  const skills = [];
  const prompts = [];
  const problems = [];
  const skillsDir = path.join(appDir, 'skills');
  if (fs.existsSync(skillsDir) && fs.statSync(skillsDir).isDirectory()) {
    for (const name of fs.readdirSync(skillsDir).sort()) {
      const dir = path.join(skillsDir, name);
      if (name === '.DS_Store' || !fs.lstatSync(dir).isDirectory()) continue;
      const rel = `skills/${name}/SKILL.md`;
      if (!fs.existsSync(path.join(dir, 'SKILL.md'))) { problems.push({ kind: 'skill_without_file', name, path: rel }); continue; }
      if (!AGENT_NAME_RE.test(name)) { problems.push({ kind: 'invalid_skill_name', name, path: rel }); continue; }
      if (isTestPath(rel)) { problems.push({ kind: 'excluded_by_test_rule', name, path: rel }); continue; }
      skills.push({ name, path: rel });
    }
  }
  const promptsDir = path.join(appDir, 'prompts');
  if (fs.existsSync(promptsDir) && fs.statSync(promptsDir).isDirectory()) {
    for (const file of fs.readdirSync(promptsDir).sort()) {
      if (!file.endsWith('.md') || !fs.lstatSync(path.join(promptsDir, file)).isFile()) continue;
      const name = file.slice(0, -3);
      const rel = `prompts/${file}`;
      if (!AGENT_NAME_RE.test(name)) { problems.push({ kind: 'invalid_prompt_name', name, path: rel }); continue; }
      if (isTestPath(rel)) { problems.push({ kind: 'excluded_by_test_rule', name, path: rel }); continue; }
      prompts.push({ name, path: rel });
    }
  }
  return { skills, prompts, problems };
}

// Ende eines Blocks ab `open` (Index der öffnenden Klammer), Strings werden übersprungen.
function blockEnd(text, open) {
  let depth = 0;
  let quote = null;
  for (let i = open; i < text.length; i += 1) {
    const c = text[i];
    if (quote) { if (c === '\\') i += 1; else if (c === quote) quote = null; continue; }
    if (c === '"' || c === "'" || c === '`') { quote = c; continue; }
    if (c === '{') depth += 1;
    else if (c === '}') { depth -= 1; if (depth === 0) return i; }
  }
  return text.length;
}

// Skill-Verweise in App-Code: `skill:`/`suggested_skill:` mit Literal (auch `x ?? '<lit>'`), `required_skills`/
// `requiredSkills` als Array, Konstanten `*SKILL* = '<lit>'`, `return '<lit>'` in Funktionen mit „skill“ im Namen.
// Liefert [{ref, line, via}] (nur Werte in Skill-Namensform; Beschriftungen wie 'Skill' fallen heraus).
export function extractSkillRefs(source) {
  const text = stripLeadingComments(source);
  const out = [];
  const add = (ref, index, via) => { if (SKILL_REF_RE.test(ref)) out.push({ ref, line: lineOf(text, index), via }); };
  for (const m of text.matchAll(/\b(?:suggested_)?skill\s*:\s*(?:[^\n:;]{0,80}?\?\?\s*)?(['"])([^'"\n]+)\1/g)) add(m[2], m.index, 'skill:');
  // Arrays auf derselben Zeile, auch hinter einem Bedingungsausdruck (`x ? ['a'] : ['b']`).
  for (const m of text.matchAll(/\b(?:required_skills|requiredSkills)\s*[:=]\s*([^\n;]{0,240})/g)) {
    for (const arr of m[1].matchAll(/\[([^\]]*)\]/g)) {
      for (const s of arr[1].matchAll(/(['"])([^'"\n]+)\1/g)) add(s[2], m.index, 'required_skills');
    }
  }
  for (const m of text.matchAll(/\b(?:const|let|var)\s+[A-Z][A-Z0-9_]*SKILL[A-Z0-9_]*\s*=\s*(['"])([^'"\n]+)\1/g)) add(m[2], m.index, 'const');
  for (const m of text.matchAll(/\bfunction\s+[A-Za-z_$]*[Ss]kill[\w$]*\s*\([^)]*\)\s*\{/g)) {
    const open = m.index + m[0].length - 1;
    const body = text.slice(open, blockEnd(text, open) + 1);
    for (const r of body.matchAll(/\breturn\s+(['"])([^'"\n]+)\1/g)) add(r[2], open + r.index, 'return');
  }
  return out.sort((a, b) => a.line - b.line || a.ref.localeCompare(b.ref));
}

// Namensraum-Verweise `<app>/<name>` im Handler-Code (handlers/**, ohne Tests).
export function scanHandlerSkillRefs(appDir) {
  const refs = [];
  const files = walk(path.join(appDir, 'handlers')).map((f) => `handlers/${f}`).filter((f) => CODE_EXT_RE.test(f) && !isTestPath(f));
  for (const rel of files) {
    for (const r of extractSkillRefs(fs.readFileSync(path.join(appDir, rel), 'utf8'))) {
      if (r.ref.includes('/')) refs.push({ ...r, file: rel });
    }
  }
  return refs;
}

// Prüfung §16.30: jeder `<app>/<name>` im Handler-Code ist im eigenen Manifest deklariert; kein fremder Namensraum.
export function checkSkillRefs(appId, skills, refs) {
  const declared = new Set(skills.map((s) => s.name));
  const violations = [];
  for (const r of refs) {
    const [ns, name] = r.ref.split('/');
    if (ns !== appId) violations.push({ kind: 'foreign_namespace', app: appId, ref: r.ref, file: r.file, line: r.line });
    else if (!declared.has(name)) violations.push({ kind: 'undeclared_skill', app: appId, ref: r.ref, file: r.file, line: r.line });
  }
  return violations;
}

// ---------------------------------------------------------------- Code-Ableitung: Befehle, Trigger, net.ingest (Strang MAN)

// Ende eines Klammerausdrucks ab `open` ((, [ oder {); Strings und Template-Literale werden übersprungen.
export function matchEnd(text, open) {
  const pairs = { '(': ')', '[': ']', '{': '}' };
  const stack = [];
  let quote = null;
  for (let i = open; i < text.length; i += 1) {
    const c = text[i];
    if (quote) { if (c === '\\') i += 1; else if (c === quote) quote = null; continue; }
    if (c === '"' || c === "'" || c === '`') { quote = c; continue; }
    if (pairs[c]) stack.push(pairs[c]);
    else if (c === ')' || c === ']' || c === '}') {
      if (stack.pop() !== c) return text.length;
      if (!stack.length) return i;
    }
  }
  return text.length;
}

// Inhalt zwischen `open` und der passenden Klammer an Kommas der obersten Ebene teilen.
export function splitTopLevel(text, open) {
  const close = matchEnd(text, open);
  const parts = [];
  let depth = 0;
  let quote = null;
  let start = open + 1;
  for (let i = open + 1; i < close; i += 1) {
    const c = text[i];
    if (quote) { if (c === '\\') i += 1; else if (c === quote) quote = null; continue; }
    if (c === '"' || c === "'" || c === '`') { quote = c; continue; }
    if (c === '(' || c === '[' || c === '{') depth += 1;
    else if (c === ')' || c === ']' || c === '}') depth -= 1;
    else if (c === ',' && depth === 0) { parts.push(text.slice(start, i).trim()); start = i + 1; }
  }
  const last = text.slice(start, close).trim();
  if (last) parts.push(last);
  return { parts, close };
}

// Eigenschaften der obersten Ebene eines Objektliterals: [[schlüssel, ausdruck]]; Kurzform `{ module }` → ['module', 'module'].
export function topLevelProps(literal) {
  const s = String(literal || '').trim();
  if (!s.startsWith('{')) return [];
  const out = [];
  for (const part of splitTopLevel(s, 0).parts) {
    const m = /^(?:([A-Za-z_$][\w$]*)|(['"])([^'"]+)\2)\s*(?::\s*([\s\S]*))?$/.exec(part);
    if (!m) continue;
    const key = m[1] ?? m[3];
    out.push([key, m[4] === undefined ? key : m[4].trim()]);
  }
  return out;
}

// Konstanten einer App (Handler-Code inkl. _lib): `const X = 'lit'`, `const O = {k: 'lit'}`, `const A = ['a', 'b']`
// (auch Object.freeze). Mehrdeutige Namen (verschiedene Werte) lösen nicht auf.
export function collectConstants(texts) {
  const str = new Map();
  const obj = new Map();
  const arr = new Map();
  const add = (map, key, value) => { if (!map.has(key)) map.set(key, new Set()); map.get(key).add(value); };
  for (const text of texts) {
    for (const m of text.matchAll(/\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(['"`])([^'"`\n$]*)\2\s*(?=[;\n,)])/g)) add(str, m[1], m[3]);
    for (const m of text.matchAll(/\bconst\s+([A-Za-z_$][\w$]*)\s*=\s*(?:Object\.freeze\(\s*)?([[{])/g)) {
      const open = m.index + m[0].length - 1;
      const literal = text.slice(open, matchEnd(text, open) + 1);
      if (m[2] === '{') {
        for (const [key, expr] of topLevelProps(literal)) {
          const lit = /^(['"`])([^'"`\n$]*)\1$/.exec(expr);
          if (lit) add(obj, `${m[1]}.${key}`, lit[2]);
        }
      } else {
        const items = splitTopLevel(literal, 0).parts;
        const lits = items.map((x) => /^(['"`])([^'"`\n$]*)\1$/.exec(x)?.[2]);
        if (items.length && lits.every((x) => x !== undefined)) add(arr, m[1], JSON.stringify(lits));
      }
    }
  }
  const single = (map) => new Map([...map].filter(([, v]) => v.size === 1).map(([k, v]) => [k, [...v][0]]));
  return { str: single(str), obj: single(obj), arr: new Map([...single(arr)].map(([k, v]) => [k, JSON.parse(v)])) };
}

export function resolveString(expr, consts) {
  const e = String(expr ?? '').trim();
  const lit = /^(['"`])([^'"`\n$]*)\1$/.exec(e);
  if (lit) return lit[2];
  if (/^[A-Za-z_$][\w$]*$/.test(e)) return consts.str.get(e) ?? null;
  if (/^[A-Za-z_$][\w$]*\.[A-Za-z_$][\w$]*$/.test(e)) return consts.obj.get(e) ?? null;
  return null;
}

export function resolveStringList(expr, consts) {
  const e = String(expr ?? '').trim();
  if (!e.startsWith('[')) return null;
  const out = [];
  for (const part of splitTopLevel(e, 0).parts) {
    const spread = /^\.\.\.\s*([A-Za-z_$][\w$.]*)$/.exec(part);
    if (spread) {
      const list = consts.arr.get(spread[1]);
      if (!list) return null;
      out.push(...list);
      continue;
    }
    const v = resolveString(part, consts);
    if (v == null) return null;
    out.push(v);
  }
  return out;
}

function handlerCodeFiles(appDir) {
  return walk(path.join(appDir, 'handlers')).map((f) => `handlers/${f}`).filter((f) => CODE_EXT_RE.test(f) && !isTestPath(f));
}

// `module` aus den Befehlsoptionen: Objektliteral oder Bezeichner eines `const x = {…}` derselben Datei.
// Ergebnis: null = kein module (eigene App), sonst der Ausdruck.
function moduleExprOfOptions(optsExpr, text) {
  let lit = String(optsExpr || '').trim();
  if (!lit) return null;
  if (/^[A-Za-z_$][\w$]*$/.test(lit)) {
    const m = new RegExp(`\\b(?:const|let)\\s+${lit.replace(/\$/g, '\\$')}\\s*=\\s*\\{`).exec(text);
    if (!m) return lit;
    const open = m.index + m[0].length - 1;
    lit = text.slice(open, matchEnd(text, open) + 1);
  }
  if (!lit.startsWith('{')) return lit;
  const hit = topLevelProps(lit).find(([k]) => k === 'module');
  return hit ? hit[1] : null;
}

// Befehle an fremde Apps aus dem Handler-Code (§16.40 commands_out): ctx.command(type, payload, {module}).
// type: Literal oder Konstante; `<param>.type` in einer exportierten _lib-Funktion = Typ jedes Handlers, der sie ruft.
// Ohne module = eigene App. Nicht auflösbare Aufrufe landen in `unresolved` (Deklaration commands_out nötig).
export function scanCommandCalls(appDir, appId, handlers, consts) {
  const out = [];
  const unresolved = [];
  const texts = Object.fromEntries(handlerCodeFiles(appDir).map((rel) => [rel, stripLeadingComments(fs.readFileSync(path.join(appDir, rel), 'utf8'))]));
  const typeOfHandler = Object.fromEntries(Object.entries(handlers).map(([t, rel]) => [rel, t]));
  for (const [rel, text] of Object.entries(texts)) {
    for (const m of text.matchAll(/\bctx\??\.command\s*\(/g)) {
      const open = m.index + m[0].length - 1;
      const { parts } = splitTopLevel(text, open);
      const line = lineOf(text, m.index);
      const typeExpr = parts[0] || '';
      const moduleExpr = moduleExprOfOptions(parts[2], text);
      const target = moduleExpr == null ? appId : resolveString(moduleExpr, consts);
      let types = [];
      const direct = resolveString(typeExpr, consts);
      if (direct) types = [direct];
      else if (/^[A-Za-z_$][\w$]*\.type$/.test(typeExpr)) {
        if (typeOfHandler[rel]) types = [typeOfHandler[rel]];
        else {
          const before = [...text.slice(0, m.index).matchAll(/export\s+(?:async\s+)?function\s+([A-Za-z_$][\w$]*)/g)].pop();
          if (before) {
            const callRe = new RegExp(`\\b${before[1].replace(/\$/g, '\\$')}\\s*\\(`);
            types = Object.entries(handlers).filter(([, hrel]) => texts[hrel] && callRe.test(texts[hrel])).map(([t]) => t).sort();
          }
        }
      }
      if (!target || !types.length) {
        unresolved.push({ file: rel, line, type_expr: typeExpr.slice(0, 80), module_expr: moduleExpr == null ? null : String(moduleExpr).slice(0, 80) });
        continue;
      }
      if (target === appId) continue;
      for (const type of types) out.push({ app: target, type, evidence: `${rel}:${line}` });
    }
  }
  return { out, unresolved };
}

// Befehle aus dem UI-Code (alle Code-Dateien außer handlers/, Tests, vendor/, dist/):
// (1) Kontext-Chat-Baustein shared/context-chat.js (sendContextChat/buildContextChatCommand) → Standardbefehle der
//     Chat-Eigentümer-App; ctox.app.modify.request nur, wenn die Datei canModifyApp weitergibt.
// (2) `module: '<andere-app>'` mit einem Literal, das ein Handler-Typ genau dieser App (und keiner der sendenden App) ist,
//     im selben Ausdruck (±15 Zeilen); api.command kennt keine Zieloption (app-api.d.ts), das Ziel steht im payload.module.
export function scanUiCommands(appDir, appId, files, chat, handlerTypesByApp) {
  const out = [];
  for (const rel of Object.keys(files).sort()) {
    if (!CODE_EXT_RE.test(rel) || rel.startsWith('handlers/') || isTestPath(rel) || /(^|\/)(vendor|dist|node_modules)\//.test(rel)) continue;
    const text = stripLeadingComments(fs.readFileSync(path.join(appDir, rel), 'utf8'));
    if (chat && appId !== chat.owner && /from\s+['"][^'"]*context-chat\.js['"]/.test(text)) {
      const call = /\b(?:sendContextChat|buildContextChatCommand)\s*\(/.exec(text);
      if (call) {
        const ev = `${rel}:${lineOf(text, call.index)}`;
        out.push({ app: chat.owner, type: chat.command, evidence: ev });
        if (/\bcanModifyApp\b/.test(text)) out.push({ app: chat.owner, type: chat.modify, evidence: ev });
      }
    }
    const lines = text.split('\n');
    for (const m of text.matchAll(/\bmodule\s*:\s*(['"])([a-z0-9][a-z0-9-]*)\1/g)) {
      const target = m[2];
      const types = handlerTypesByApp[target];
      if (target === appId || !types) continue;
      const line = lineOf(text, m.index);
      const window = lines.slice(Math.max(0, line - 16), line + 15).join('\n');
      for (const s of window.matchAll(/(['"])([a-z][a-z0-9_.-]*)\1/g)) {
        // Hat die sendende App selbst einen Handler dieses Typs, routet der Core (app_id, type) zu ihr (§16.25);
        // das module-Feld ist dann Altlast (z. B. OLG sendet outbound.* mit module 'outbound').
        if (types.has(s[2]) && !handlerTypesByApp[appId]?.has(s[2])) out.push({ app: target, type: s[2], evidence: `${rel}:${line}` });
      }
    }
  }
  return out;
}

// ROUTE (E39, Präzisierung §16.40): Befehlstypen aus dem UI-Code, die über api.command gehen.
// Kommentare werden durch Leerzeichen ersetzt (Offsets und Zeilen bleiben), Strings/Template-Literale/Regex-Literale
// bleiben unangetastet.
export function blankComments(text) {
  const out = text.split('');
  let quote = null;
  let prevSig = '';
  for (let i = 0; i < text.length; i += 1) {
    const c = text[i];
    if (quote) {
      if (c === '\\') { i += 1; continue; }
      if (quote === '/' && c === '[') { quote = ']'; continue; }
      if (quote === ']' && c === ']') { quote = '/'; continue; }
      if (c === quote || (quote !== '`' && quote !== '/' && quote !== ']' && c === '\n')) { quote = null; prevSig = 'x'; }
      continue;
    }
    if (c === '/' && text[i + 1] === '/') {
      while (i < text.length && text[i] !== '\n') { out[i] = ' '; i += 1; }
      continue;
    }
    if (c === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2);
      const stop = end < 0 ? text.length : end + 2;
      for (; i < stop; i += 1) if (text[i] !== '\n') out[i] = ' ';
      i -= 1;
      continue;
    }
    if (c === '"' || c === "'" || c === '`') { quote = c; continue; }
    if (c === '/' && (!prevSig || /[(,=:[!&|?{};+\-*%<>~^]/.test(prevSig) || /\breturn$/.test(text.slice(Math.max(0, i - 8), i).trimEnd()))) { quote = '/'; continue; }
    if (!/\s/.test(c)) prevSig = c;
  }
  return out.join('');
}

export const COMMAND_TYPE_RE = /^[a-z][a-z0-9_-]*(?:\.[a-z0-9_-]+)+$/;
const API_COMMAND_RE = /(?<![\w$])(?:[\w$]+\??\.)*[\w$]*[aA]pi\??\.command\s*\(/g;
const NOT_WRAPPER = new Set(['if', 'for', 'while', 'switch', 'catch', 'function', 'return', 'typeof', 'await', 'new', 'constructor']);

// Funktionsdefinitionen einer Datei: function f(…){…}, const f = (…) => {…}, const f = function (…){…}, Methode f(…){…}.
// Ergebnis: [{name, index, params:[…], body}].
function functionDefs(text) {
  const defs = [];
  const res = [
    /\bfunction\s*\*?\s*([A-Za-z_$][\w$]*)\s*\(/g,
    /\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s+)?(?:function\b[^(]*)?\(/g,
    /\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s+)?([A-Za-z_$][\w$]*)\s*=>/g,
    /(?:^|[\n;{},])[ \t]*(?:async\s+)?([A-Za-z_$][\w$]*)\s*\((?=[^)]*\)\s*\{)/g,
  ];
  for (const [ri, re] of res.entries()) {
    for (const m of text.matchAll(re)) {
      const name = m[1];
      if (NOT_WRAPPER.has(name)) continue;
      let i = m.index + m[0].length;
      let params;
      if (ri === 2) params = [m[2]];
      else {
        params = splitTopLevel(text, i - 1).parts.map((x) => String(x).trim().replace(/\s*=[\s\S]*$/, '').replace(/^\.\.\./, ''));
        i = matchEnd(text, i - 1) + 1;
      }
      const rest = /^\s*(?:=>\s*)?/.exec(text.slice(i))[0];
      const arrow = ri === 2 || /=>/.test(rest);
      i += rest.length;
      if (text[i] !== '{') {
        if (!arrow) continue;
        // Ausdruckskörper: bis zum Zeilenende.
        const nl = text.indexOf('\n', i);
        defs.push({ name, index: m.index, params, body: text.slice(i, nl < 0 ? text.length : nl) });
        continue;
      }
      if (ri === 3 && arrow) continue;
      defs.push({ name, index: m.index, params, body: text.slice(i, matchEnd(text, i) + 1) });
    }
  }
  return defs;
}

// Literale eines Ausdrucks (Strings in '…' oder "…"), die die Form eines Befehlstyps haben.
// Schlüssel eines Objektliterals (`{ 'a.b': … }`, `, 'a.b': …`) zählen nicht (Abbildungstabellen `{…}[action]`).
function typeLiterals(expr) {
  const e = String(expr);
  return [...e.matchAll(/(['"])([^'"\n]*)\1/g)]
    .filter((m) => COMMAND_TYPE_RE.test(m[2]))
    .filter((m) => !(/[{,]\s*$/.test(e.slice(0, m.index)) && /^\s*:/.test(e.slice(m.index + m[0].length))))
    .map((m) => ({ value: m[2], offset: m.index }));
}

function escapeRe(s) {
  return s.replace(/[$]/g, '\\$');
}

// Ausdruck einer Zuweisung `const|let|var <id> = …` bis zum Anweisungsende (oberste Ebene).
function assignedExpr(text, id, before) {
  const decl = [...text.slice(0, before).matchAll(new RegExp(`\\b(?:const|let|var)\\s+${escapeRe(id)}\\s*=(?!=)`, 'g'))].pop();
  if (!decl) return null;
  const start = decl.index + decl[0].length;
  let end = start;
  for (let depth = 0, q = null; end < text.length; end += 1) {
    const c = text[end];
    if (q) { if (c === '\\') end += 1; else if (c === q) q = null; continue; }
    if (c === '"' || c === "'" || c === '`') { q = c; continue; }
    if ('([{'.includes(c)) depth += 1;
    else if (')]}'.includes(c)) { if (depth === 0) break; depth -= 1; }
    else if (depth === 0 && (c === ';' || (c === '\n' && !/[?:,&|+=]\s*$/.test(text.slice(start, end)) && !/^\s*[?:&|+.]/.test(text.slice(end + 1, end + 40))))) break;
  }
  return { start, expr: text.slice(start, end) };
}

const TYPE_FIELDS = ['type', 'command_type', 'commandType'];

// UI-Befehle einer App (alle Code-Dateien außer handlers/, Tests, vendor/, dist/; bei der Shell die Shell-Bäume).
// Typ-Position = erstes Argument von api.command(…) bzw. das Argument (oder Feld type/command_type/commandType bzw. ein
// destrukturiertes Feld eines Arguments) einer app-lokalen Hüllfunktion, das (transitiv, über alle Dateien der App)
// unverändert in die Typ-Position von api.command fließt. Der Ausdruck an der Typ-Position zählt mit allen Literalen
// (auch `a ? 'x' : 'y'`); ein bloßer Bezeichner wird über die letzte `const|let|var <id> = …` derselben Datei vor dem
// Aufruf bzw. eine eindeutige Konstante der App aufgelöst. Dazu `command_type:`/`commandType:` mit Literal.
// Ergebnis: [{type, file, line}] (eindeutig je type+Fundstelle).
export function scanUiCommandTypes(rootDir, rels) {
  const texts = {};
  for (const rel of rels) texts[rel] = blankComments(fs.readFileSync(path.join(rootDir, rel), 'utf8'));
  const defs = Object.fromEntries(Object.entries(texts).map(([rel, t]) => [rel, functionDefs(t)]));
  const specs = new Map(); // Hüllfunktion → {k, field}
  const argAt = (text, open, spec) => {
    const part = String(splitTopLevel(text, open).parts[spec.k] ?? '').trim();
    if (!spec.field) return part;
    const hit = topLevelProps(part).find(([key]) => (spec.field === '*' ? TYPE_FIELDS.includes(key) : key === spec.field));
    return hit ? hit[1] : (spec.field !== '*' || !part.startsWith('{') ? '' : '');
  };
  const callRe = (name) => new RegExp(`(?<![\\w$])(?:[\\w$]+\\??\\.)?${escapeRe(name)}\\s*\\(`, 'g');
  for (let changed = true; changed;) {
    changed = false;
    for (const list of Object.values(defs)) {
      for (const d of list) {
        if (specs.has(d.name)) continue;
        const inner = [...d.body.matchAll(new RegExp(API_COMMAND_RE.source, 'g'))].map((m) => argAt(d.body, m.index + m[0].length - 1, { k: 0 }));
        for (const [w, spec] of specs) {
          if (w === d.name) continue;
          for (const m of d.body.matchAll(callRe(w))) inner.push(argAt(d.body, m.index + m[0].length - 1, spec));
        }
        let spec = null;
        d.params.forEach((p, k) => {
          if (spec || !p) return;
          if (/^[A-Za-z_$][\w$]*$/.test(p)) {
            if (inner.includes(p)) spec = { k, field: null };
            else if (inner.some((e) => new RegExp(`^${escapeRe(p)}\\??\\.(?:${TYPE_FIELDS.join('|')})$`).test(e))) spec = { k, field: '*' };
          } else if (p.startsWith('{')) {
            const names = topLevelProps(p).map(([key, v]) => [key, v.replace(/\s*=[\s\S]*$/, '').trim()]);
            const hit = names.find(([, local]) => inner.includes(local));
            if (hit) spec = { k, field: hit[0] };
          }
        });
        if (spec) { specs.set(d.name, spec); changed = true; }
      }
    }
  }
  const appConsts = new Map();
  for (const t of Object.values(texts)) {
    for (const m of t.matchAll(/\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(['"])([^'"\n]*)\2/g)) {
      if (!appConsts.has(m[1])) appConsts.set(m[1], new Set());
      appConsts.get(m[1]).add(m[3]);
    }
  }
  const hits = new Map();
  const add = (type, rel, text, index) => {
    const line = lineOf(text, index);
    hits.set(`${type} ${rel}:${line}`, { type, file: rel, line });
  };
  // Literale eines Ausdrucks an der Typ-Position; bloßer Bezeichner → Zuweisung davor bzw. Konstante der App.
  const collect = (rel, text, expr, at, before, field) => {
    let e = String(expr || '').trim();
    if (!e) return;
    if (field) {
      if (/^[A-Za-z_$][\w$]*$/.test(e)) {
        const a = assignedExpr(text, e, before);
        if (!a) return;
        e = a.expr.trim();
        at = a.start;
      }
      const hit = topLevelProps(e).find(([key]) => (field === '*' ? TYPE_FIELDS.includes(key) : key === field));
      if (!hit) return;
      e = hit[1];
    }
    const start = Math.max(0, text.indexOf(e, at));
    for (const lit of typeLiterals(e)) add(lit.value, rel, text, start + lit.offset);
    if (!/^[A-Za-z_$][\w$]*$/.test(e)) return;
    const a = assignedExpr(text, e, before);
    if (a) {
      for (const lit of typeLiterals(a.expr)) add(lit.value, rel, text, a.start + lit.offset);
    } else if (appConsts.get(e)?.size === 1) {
      const v = [...appConsts.get(e)][0];
      if (COMMAND_TYPE_RE.test(v)) add(v, rel, text, before);
    }
  };
  const wrapperRe = specs.size
    ? new RegExp(`(?<![\\w$])(?:[\\w$]+\\??\\.)?(${[...specs.keys()].map(escapeRe).join('|')})\\s*\\(`, 'g')
    : null;
  for (const [rel, text] of Object.entries(texts)) {
    const defIndex = defs[rel].map((d) => d.index);
    const sites = [...text.matchAll(new RegExp(API_COMMAND_RE.source, 'g'))].map((m) => ({ m, spec: { k: 0, field: null } }));
    if (wrapperRe) {
      for (const m of text.matchAll(wrapperRe)) {
        if (/\bfunction\s*\*?\s*$/.test(text.slice(Math.max(0, m.index - 12), m.index))) continue;
        // Definition `name(…) {` bzw. `const name = (…) =>` statt Aufruf.
        const after = text.slice(matchEnd(text, m.index + m[0].length - 1) + 1);
        if (defIndex.some((s) => s <= m.index && m.index - s <= 24 && !/[;()]/.test(text.slice(s + 1, m.index))) && /^\s*(?:\{|=>)/.test(after)) continue;
        sites.push({ m, spec: specs.get(m[1]) });
      }
    }
    for (const { m, spec } of sites) {
      const open = m.index + m[0].length - 1;
      const part = String(splitTopLevel(text, open).parts[spec.k] ?? '');
      collect(rel, text, part, open, m.index, spec.field);
    }
    for (const m of text.matchAll(/\b(?:command_type|commandType)\s*:\s*(['"])([^'"\n]*)\1/g)) {
      // E42: ein Konfigurationsobjekt mit dispatch:false sendet den Typ nicht (z. B. universal-importer in OLG).
      if (COMMAND_TYPE_RE.test(m[2]) && !enclosingObjectHasDispatchFalse(text, m.index)) add(m[2], rel, text, m.index);
    }
  }
  return [...hits.values()].sort((a, b) => a.type.localeCompare(b.type) || a.file.localeCompare(b.file) || a.line - b.line);
}

// E39/E40: UI-Befehlstypen je Absender einordnen. Eigener Handler → eigene Route; Plattformroute → nichts zu
// deklarieren; genau eine fremde App mit Handler → commands_out (+ commands_in beim Ziel); mehrere → mehrdeutig (rot);
// keine → PLATFORM_ROUTES_PENDING (Bericht) bzw. commands_unrouted (rot). out[].evidence = <datei>:<zeile> relativ zur
// sendenden App (wie _commands_out_evidence), out[].source und Drift-Fundstellen = <app-rel>/<datei>:<zeile>.
export function routeUiCommands({ senders, handlerTypesByApp, routes, pending = PLATFORM_ROUTES_PENDING }) {
  const out = [];
  const group = new Map();
  const push = (bucket, key, base, ev) => {
    if (!group.has(bucket)) group.set(bucket, new Map());
    const g = group.get(bucket);
    if (!g.has(key)) g.set(key, { ...base, evidence: [] });
    g.get(key).evidence.push(ev);
  };
  for (const s of senders) {
    for (const h of s.hits) {
      const ev = `${s.rel ? `${s.rel}/` : ''}${h.file}:${h.line}`;
      if (handlerTypesByApp[s.id]?.has(h.type)) continue;
      // E40/E42: Plattformroute vor fremdem Handler – der Kern führt diese Typen nativ aus, ohne Deklaration.
      if (isPlatformRoute(h.type, routes)) continue;
      const owners = Object.keys(handlerTypesByApp).filter((id) => id !== s.id && handlerTypesByApp[id].has(h.type)).sort();
      if (owners.length === 1) out.push({ from: s.id, app: owners[0], type: h.type, evidence: `${h.file}:${h.line}`, source: ev });
      else if (owners.length > 1) push('ambiguous', `${s.id} ${h.type}`, { app: s.id, type: h.type, handlers: owners }, ev);
      else if (pending[h.type]) push('pending', `${s.id} ${h.type}`, { app: s.id, type: h.type, basis: pending[h.type] }, ev);
      else push('unrouted', `${s.id} ${h.type}`, { app: s.id, type: h.type }, ev);
    }
  }
  const list = (bucket) => [...(group.get(bucket)?.values() || [])].map((d) => ({ ...d, evidence: sortedUnique(d.evidence) }))
    .sort((a, b) => a.app.localeCompare(b.app) || a.type.localeCompare(b.type));
  // Jeder Eintrag der Tabelle erscheint im Bericht, auch ohne Fundstelle im Scan.
  const pendingList = list('pending');
  for (const [type, basis] of Object.entries(pending)) {
    if (!pendingList.some((d) => d.type === type)) pendingList.push({ app: null, type, basis, evidence: [] });
  }
  pendingList.sort((a, b) => a.type.localeCompare(b.type) || String(a.app).localeCompare(String(b.app)));
  out.sort((a, b) => a.from.localeCompare(b.from) || a.app.localeCompare(b.app) || a.type.localeCompare(b.type) || a.evidence.localeCompare(b.evidence));
  return { out, ambiguous: list('ambiguous'), unrouted: list('unrouted'), pending: pendingList };
}

// UI-Code der Shell (§16.15: v4/frame, v4/shell, v4/source) aus der Dateiliste des Shell-Manifests, ohne Tests/Labore.
export function shellUiFiles(files) {
  return Object.keys(files).sort().filter((rel) => SHELL_TREES.some((t) => rel.startsWith(`${t}/`)) && CODE_EXT_RE.test(rel)
    && !isTestPath(rel) && !/(^|\/)(lab|tests?|vendor|dist|node_modules)\//.test(rel));
}

// UI-Code-Dateien einer App (wie scanUiCommands).
export function uiCodeFiles(files) {
  return Object.keys(files).sort().filter((rel) => CODE_EXT_RE.test(rel) && !rel.startsWith('handlers/') && !isTestPath(rel) && !/(^|\/)(vendor|dist|node_modules)\//.test(rel));
}

// Kontext-Chat-Konstanten aus shared/context-chat.js (Quelle des Bausteins, nicht abgeschrieben).
export function loadContextChat(appsRoot) {
  const file = path.join(appsRoot, 'shared/context-chat.js');
  if (!fs.existsSync(file)) return null;
  const consts = collectConstants([fs.readFileSync(file, 'utf8')]);
  const owner = consts.str.get('CONTEXT_CHAT_OWNER');
  const command = consts.str.get('CONTEXT_CHAT_COMMAND');
  const modify = consts.str.get('APP_MODIFY_COMMAND');
  return owner && command && modify ? { owner, command, modify } : null;
}

// Trigger als Code-Export eines Handlers: `export const triggers = [{on:{collection, op}} | {every:'PT…'}]` (reines
// Literal, ohne Bezeichner); handler = die Datei selbst. Fehler (kein Literal) → errors.
export function scanTriggerExports(appDir, handlers) {
  const triggers = [];
  const errors = [];
  for (const rel of Object.values(handlers).sort()) {
    const text = stripLeadingComments(fs.readFileSync(path.join(appDir, rel), 'utf8'));
    const m = /export\s+const\s+triggers\s*=\s*(?:Object\.freeze\(\s*)?\[/.exec(text);
    if (!m) continue;
    const open = m.index + m[0].length - 1;
    let value;
    try {
      value = vm.runInNewContext(`(${text.slice(open, matchEnd(text, open) + 1)})`, Object.create(null), { timeout: 2000 });
    } catch (err) {
      errors.push({ file: rel, detail: `triggers ist kein reines Literal (${String(err.message).split('\n')[0]})` });
      continue;
    }
    for (const t of Array.isArray(value) ? value : []) {
      const entry = t && typeof t === 'object' ? { handler: rel, ...(t.on ? { on: { collection: t.on.collection, op: t.on.op } } : {}), ...(t.every ? { every: t.every } : {}) } : null;
      const problem = triggerShapeError(entry);
      if (problem) errors.push({ file: rel, detail: problem });
      else triggers.push({ ...entry, _source: `${rel} export triggers` });
    }
  }
  return { triggers, errors };
}

// net.ingest-Anmeldungen (§16.38, Ergänzung §16.39: command = Befehlstyp der Zustellung) aus dem Handler-Code.
export function scanNetIngest(appDir, consts) {
  const out = [];
  for (const rel of handlerCodeFiles(appDir)) {
    const text = stripLeadingComments(fs.readFileSync(path.join(appDir, rel), 'utf8'));
    for (const m of text.matchAll(/\bnet\??\.ingest\??\.webhook\??\.register\s*\(/g)) {
      const open = m.index + m[0].length - 1;
      const props = Object.fromEntries(topLevelProps(splitTopLevel(text, open).parts[0]));
      const entry = {
        command: resolveString(props.command, consts),
        path: resolveString(props.path, consts),
        _source: `${rel}:${lineOf(text, m.index)}`,
      };
      const aliases = props.aliases ? resolveStringList(props.aliases, consts) : null;
      if (aliases) entry.aliases = aliases;
      const passHeaders = props.pass_headers ? resolveStringList(props.pass_headers, consts) : null;
      if (passHeaders) entry.pass_headers = passHeaders;
      out.push(entry);
    }
  }
  return out.sort((a, b) => String(a.command).localeCompare(String(b.command)) || a._source.localeCompare(b._source));
}

// Konfigurationsschlüssel aus `config.get({key: …})` im Handler-Code (Anmerkung; der Kernvertrag kennt noch kein Feld).
// Hüllfunktionen (`runtimeConfig(ctx, key)` → `config.get({ key })`, `configFlag(ctx, key)` → `runtimeConfig(ctx, key)`)
// werden bis zum Fixpunkt verfolgt; der Schlüssel ist dann das zweite Argument des Aufrufs.
export function scanConfigKeys(appDir, consts) {
  const keys = new Set();
  const texts = handlerCodeFiles(appDir).map((rel) => stripLeadingComments(fs.readFileSync(path.join(appDir, rel), 'utf8')));
  const all = texts.join('\n');
  const wrappers = new Set();
  const fnRe = /(?:function\s+([A-Za-z_$][\w$]*)\s*\(|(?:const|let)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\([^)]*\)\s*=>)/g;
  for (let changed = true; changed;) {
    changed = false;
    const defs = [...all.matchAll(fnRe)];
    defs.forEach((d, i) => {
      const name = d[1] || d[2];
      if (wrappers.has(name)) return;
      const body = all.slice(d.index, defs[i + 1]?.index ?? all.length);
      const direct = /\bconfig\??\.get\s*\(\s*\{\s*key\s*[,}]/.test(body);
      const viaWrapper = [...wrappers].some((w) => new RegExp(`\\b${w.replace(/\$/g, '\\$')}\\s*\\(\\s*ctx\\s*,\\s*key\\b`).test(body));
      if (direct || viaWrapper) { wrappers.add(name); changed = true; }
    });
  }
  for (const text of texts) {
    for (const m of text.matchAll(/\bconfig\??\.get\s*\(\s*\{\s*key\s*:\s*([^,}\n]+)/g)) {
      const v = resolveString(m[1], consts);
      if (v) keys.add(v);
    }
    for (const w of wrappers) {
      for (const m of text.matchAll(new RegExp(`\\b${w.replace(/\$/g, '\\$')}\\s*\\(\\s*ctx\\s*,\\s*([^,)\\n]+)`, 'g'))) {
        const v = resolveString(m[1], consts);
        if (v) keys.add(v);
      }
    }
  }
  return [...keys].sort();
}

// ---------------------------------------------------------------- Deklarationsdatei laden und prüfen

export function triggerShapeError(t) {
  if (!t || typeof t !== 'object') return 'Trigger ist kein Objekt';
  if (typeof t.handler !== 'string' || !/^handlers\/[^/_][^/]*\.js$/.test(t.handler)) return 'handler muss handlers/<typ>.js sein';
  const hasOn = t.on !== undefined;
  const hasEvery = t.every !== undefined;
  if (hasOn === hasEvery) return 'genau eines von on/every';
  if (hasOn) {
    if (!t.on || typeof t.on.collection !== 'string' || !['insert', 'update', 'delete'].includes(t.on.op)) return 'on braucht {collection, op: insert|update|delete}';
    return null;
  }
  const m = /^PT(?:(\d+)H)?(?:(\d+)M)?$/.exec(String(t.every));
  if (!m || (!m[1] && !m[2]) || Number(m[1] || 0) * 60 + Number(m[2] || 0) < 1) return 'every braucht ISO-Dauer ≥ PT1M (Stunden/Minuten)';
  return null;
}

export function validateDeclaration(decl, appId) {
  const errors = [];
  const err = (kind, detail) => errors.push({ app: appId, kind, detail });
  if (!decl || typeof decl !== 'object' || Array.isArray(decl)) { err('declaration_not_object', DECLARATION_FILE); return errors; }
  if (decl._format !== DECLARATION_FORMAT) err('declaration_format', `_format muss ${DECLARATION_FORMAT} sein`);
  for (const k of Object.keys(decl)) if (!DECLARATION_KEYS.includes(k)) err('declaration_unknown_key', k);
  const opsOk = (ops) => Array.isArray(ops) && ops.length > 0 && ops.every((o) => VALID_OPS.includes(o));
  const obj = (v) => v && typeof v === 'object' && !Array.isArray(v);
  for (const key of ['foreign_collections', 'exports']) {
    if (decl[key] === undefined) continue;
    if (!obj(decl[key])) { err('declaration_shape', `${key} muss ein Objekt sein`); continue; }
    for (const [n, ops] of Object.entries(decl[key])) if (!opsOk(ops)) err('declaration_ops', `${key}.${n}: ops ⊆ read|write|watch`);
  }
  if (decl.foreign_drop !== undefined && !(Array.isArray(decl.foreign_drop) && decl.foreign_drop.every((x) => typeof x === 'string'))) err('declaration_shape', 'foreign_drop muss eine Namensliste sein');
  if (decl.foreign_exclusive !== undefined && typeof decl.foreign_exclusive !== 'boolean') err('declaration_shape', 'foreign_exclusive muss boolean sein');
  for (const [i, c] of (Array.isArray(decl.commands_in) ? decl.commands_in : []).entries()) {
    if (typeof c?.type !== 'string' || !c.type) err('declaration_commands_in', `[${i}].type fehlt`);
    const fromOk = c?.from === '*' || (Array.isArray(c?.from) && c.from.length > 0 && c.from.every((a) => typeof a === 'string' && a && a !== '*' && a !== appId));
    if (!fromOk) err('declaration_commands_in', `[${i}].from muss '*' oder eine Liste fremder app_ids sein`);
  }
  if (decl.commands_in !== undefined && !Array.isArray(decl.commands_in)) err('declaration_shape', 'commands_in muss eine Liste sein');
  for (const [i, c] of (Array.isArray(decl.commands_out) ? decl.commands_out : []).entries()) {
    if (typeof c?.app !== 'string' || !c.app || typeof c?.type !== 'string' || !c.type) err('declaration_commands_out', `[${i}] braucht {app, type}`);
    else if (c.app === appId) err('declaration_commands_out', `[${i}] Ziel ist die eigene App`);
  }
  if (decl.commands_out !== undefined && !Array.isArray(decl.commands_out)) err('declaration_shape', 'commands_out muss eine Liste sein');
  for (const [i, t] of (Array.isArray(decl.triggers) ? decl.triggers : []).entries()) {
    const problem = triggerShapeError(t);
    if (problem) err('declaration_trigger', `[${i}] ${problem}`);
  }
  if (decl.triggers !== undefined && !Array.isArray(decl.triggers)) err('declaration_shape', 'triggers muss eine Liste sein');
  if (decl.multi_instance !== undefined && typeof decl.multi_instance !== 'boolean') err('declaration_shape', 'multi_instance muss boolean sein');
  if (decl.styles !== undefined && !(Array.isArray(decl.styles) && decl.styles.every((s) => typeof s === 'string'))) err('declaration_shape', 'styles muss eine Pfadliste sein');
  for (const key of ['command_rights', 'collection_rights', 'collection_overrides', 'row_filters', 'write_guard', 'class_proposals']) {
    if (decl[key] !== undefined && !obj(decl[key])) err('declaration_shape', `${key} muss ein Objekt sein`);
  }
  return errors;
}

export function loadDeclaration(appDir, appId) {
  const file = path.join(appDir, DECLARATION_FILE);
  if (!fs.existsSync(file)) return { decl: {}, present: false, errors: [] };
  let decl;
  try { decl = readJson(file); } catch (e) { return { decl: {}, present: true, errors: [{ app: appId, kind: 'declaration_unreadable', detail: String(e.message).split('\n')[0] }] }; }
  const errors = validateDeclaration(decl, appId);
  return { decl: errors.length ? {} : decl, present: true, errors };
}

// ---------------------------------------------------------------- Schemavergleich

function cmpSchemas(a, b) {
  const diffs = [];
  const eq = (x, y) => JSON.stringify(canon(x)) === JSON.stringify(canon(y));
  for (const key of ['version', 'primaryKey', 'type', 'additionalProperties']) {
    if (!eq(a[key], b[key])) diffs.push({ field: `$.${key}`, a: a[key] ?? null, b: b[key] ?? null });
  }
  const reqA = sortedUnique(a.required || []);
  const reqB = sortedUnique(b.required || []);
  if (!eq(reqA, reqB)) diffs.push({ field: '$.required', only_a: reqA.filter((x) => !reqB.includes(x)), only_b: reqB.filter((x) => !reqA.includes(x)) });
  const norm = (ix) => sortedUnique((ix || []).map((i) => (Array.isArray(i) ? i.join('+') : String(i))));
  const ixA = norm(a.indexes);
  const ixB = norm(b.indexes);
  if (!eq(ixA, ixB)) diffs.push({ field: '$.indexes', only_a: ixA.filter((x) => !ixB.includes(x)), only_b: ixB.filter((x) => !ixA.includes(x)) });
  const pA = a.properties || {};
  const pB = b.properties || {};
  for (const name of sortedUnique([...Object.keys(pA), ...Object.keys(pB)])) {
    if (!(name in pB)) diffs.push({ field: `properties.${name}`, change: 'only_a', a: pA[name] });
    else if (!(name in pA)) diffs.push({ field: `properties.${name}`, change: 'only_b', b: pB[name] });
    else if (!eq(pA[name], pB[name])) diffs.push({ field: `properties.${name}`, change: 'differs', a: pA[name], b: pB[name] });
  }
  for (const key of sortedUnique([...Object.keys(a), ...Object.keys(b)])) {
    if (['version', 'primaryKey', 'type', 'additionalProperties', 'required', 'indexes', 'properties'].includes(key)) continue;
    if (!eq(a[key], b[key])) diffs.push({ field: `$.${key}`, a: a[key] ?? null, b: b[key] ?? null });
  }
  return diffs;
}

// ---------------------------------------------------------------- Code-Nutzung

function scanCode(appDir, files, names) {
  const nameSet = new Set(names);
  const generic = new Set(names.filter((n) => !n.includes('_')));
  const hits = {};
  for (const rel of Object.keys(files).sort()) {
    if (!CODE_EXT_RE.test(rel) || rel === 'schema.js') continue;
    const lines = fs.readFileSync(path.join(appDir, rel), 'utf8').split('\n');
    lines.forEach((line, i) => {
      const found = new Set();
      for (const m of line.matchAll(/[a-z][a-z0-9_]*/g)) {
        const tok = m[0];
        if (!nameSet.has(tok)) continue;
        if (generic.has(tok)) {
          const before = line.slice(Math.max(0, m.index - 13), m.index);
          const after = line[m.index + tok.length] || '';
          const quoted = /['"`]$/.test(before) && /['"`]/.test(after);
          const access = /(collections|db|rxdb)\.$/.test(before) || /collections\[['"]$/.test(before);
          if (!quoted && !access) continue;
        }
        found.add(tok);
      }
      for (const tok of found) {
        hits[tok] ||= { read: [], write: [] };
        const ref = `${rel}:${i + 1}`;
        if (WRITE_METHOD_RE.test(line)) hits[tok].write.push(ref);
        else hits[tok].read.push(ref);
      }
    });
  }
  return hits;
}

// ---------------------------------------------------------------- Kern

function discoverApps(repo) {
  const appsDir = path.join(repo, 'src/apps/business-os');
  const list = [];
  for (const root of APP_ROOTS) {
    const dir = path.join(appsDir, root);
    if (!fs.existsSync(dir)) continue;
    for (const d of fs.readdirSync(dir).sort()) {
      const mj = path.join(dir, d, 'module.json');
      if (fs.existsSync(mj)) list.push({ dir: path.join(dir, d), rel: `${root}/${d}`, moduleJsonFile: mj });
    }
  }
  return list;
}

export async function generate(options = {}) {
  const repo = path.resolve(options.repo || DEFAULT_REPO);
  const baseRelease = options.baseRelease || BASE_RELEASE;
  const analysisDir = options.analysis || DEFAULT_ANALYSIS;
  const appsRoot = path.join(repo, 'src/apps/business-os');
  const contractFile = path.join(repo, 'src/core/business_os/business_os_schema_contract.json');
  const contract = fs.existsSync(contractFile) ? readJson(contractFile) : {};
  const baselineFile = options.baseline === undefined ? DEFAULT_BASELINE : options.baseline;
  const baseline = baselineFile && fs.existsSync(baselineFile) ? readJson(baselineFile).owners || {} : {};
  const platform = loadPlatform(options.platformManifest === undefined ? DEFAULT_PLATFORM_MANIFEST : options.platformManifest);
  const demandOnly = loadDemandOnly(repo);
  const analysis = loadAnalysisSections(analysisDir);

  // MAN2: Raster-Icons vor dem Hashen ins Paket kopieren (nur Schreiblauf, options.syncIcons) bzw. prüfen.
  const discovered = discoverApps(repo);
  const iconSelection = await loadIconSelection(appsRoot);
  const iconSync = syncPackageIcons(appsRoot, discovered, iconSelection, { write: options.syncIcons === true });

  const apps = {};
  for (const found of discovered) {
    const moduleJson = readJson(found.moduleJsonFile);
    const id = moduleJson.id || path.basename(found.dir);
    const corrected = path.join(appsRoot, 'v4/schemas', `${id}.collections.schema.json`);
    const own = path.join(found.dir, 'collections.schema.json');
    let schemaSource = null;
    let raw = { collections: {} };
    if (fs.existsSync(corrected)) { raw = readJson(corrected); schemaSource = `v4/schemas/${id}.collections.schema.json`; }
    else if (fs.existsSync(own)) { raw = readJson(own); schemaSource = `${found.rel}/collections.schema.json`; }
    const jsonSchema = normalizeCollectionsSchema(raw);
    const schemaJs = await loadSchemaJs(found.dir, appsRoot);
    const files = hashFiles(found.dir);
    const handlers = listHandlers(found.dir);
    const code = scanHandlerCode(found.dir, handlers);
    const agent = collectAgentPackage(found.dir);
    agent.violations = [
      ...agent.problems.map((p) => ({ ...p, app: id })),
      ...checkSkillRefs(id, agent.skills, scanHandlerSkillRefs(found.dir)),
    ];
    const declaration = loadDeclaration(found.dir, id);
    const consts = collectConstants(handlerCodeFiles(found.dir).map((rel) => stripLeadingComments(fs.readFileSync(path.join(found.dir, rel), 'utf8'))));
    const triggerExports = scanTriggerExports(found.dir, handlers);
    apps[id] = {
      id, dir: found.dir, rel: found.rel, moduleJson, jsonSchema, schemaSource, schemaJs: schemaJs.value, schemaJsError: schemaJs.error,
      files, handlers, code, agent,
      decl: declaration.decl, declPresent: declaration.present,
      declErrors: [...declaration.errors, ...triggerExports.errors.map((e) => ({ app: id, kind: 'trigger_export', detail: `${e.file}: ${e.detail}` }))],
      consts, codeTriggers: triggerExports.triggers,
      netIngest: scanNetIngest(found.dir, consts), configKeys: scanConfigKeys(found.dir, consts),
    };
  }
  const appIds = Object.keys(apps).sort();
  const handlerTypesByApp = Object.fromEntries(appIds.map((id) => [id, new Set(Object.keys(apps[id].handlers))]));
  const contextChat = loadContextChat(appsRoot);
  for (const app of Object.values(apps)) {
    const calls = scanCommandCalls(app.dir, app.id, app.handlers, app.consts);
    app.commandsOutCode = [...calls.out, ...scanUiCommands(app.dir, app.id, app.files, contextChat, handlerTypesByApp)];
    app.commandsOutUnresolved = calls.unresolved;
    app.uiCommandTypes = scanUiCommandTypes(app.dir, uiCodeFiles(app.files));
    // Trigger: Code-Export vor Deklaration; gleiche (handler, on|every) nur einmal.
    const seen = new Set();
    app.triggers = [];
    for (const t of [...app.codeTriggers, ...(app.decl.triggers || []).map((x) => ({ ...x, _source: DECLARATION_FILE }))]) {
      const key = JSON.stringify([t.handler, t.on ?? null, t.every ?? null]);
      if (seen.has(key)) continue;
      seen.add(key);
      app.triggers.push(t);
    }
  }

  // ---- Definierer
  const jsDefiners = {};
  const jsonDefiners = {};
  for (const app of Object.values(apps)) {
    const js = app.schemaJs;
    if (js) {
      for (const name of Object.keys(js.collections)) {
        const via = js.imports.find((imp) => apps[imp]?.schemaJs && name in apps[imp].schemaJs.collections);
        if (!via) (jsDefiners[name] ||= []).push(app.id);
      }
    }
    for (const name of Object.keys(app.jsonSchema.collections)) (jsonDefiners[name] ||= []).push(app.id);
  }

  const universe = sortedUnique([
    ...Object.keys(contract),
    ...Object.keys(baseline),
    ...Object.keys(OWNER_RESOLUTIONS),
    ...PLATFORM_COLLECTIONS,
    ...platform.collections,
    ...Object.keys(jsonDefiners),
    ...Object.values(apps).flatMap((a) => a.moduleJson.collections || []),
  ]);

  const prefixOwner = (name, candidates = null) => {
    for (const [prefix, owner] of PREFIX_OWNERS) {
      if (!name.startsWith(prefix)) continue;
      if (candidates && !candidates.includes(owner)) continue;
      return owner;
    }
    return null;
  };
  const known = (owner) => owner === PLATFORM_OWNER || Boolean(apps[owner]);

  const ownership = {};
  for (const name of universe) {
    const js = sortedUnique(jsDefiners[name] || []);
    const json = sortedUnique(jsonDefiners[name] || []);
    const inContract = name in contract;
    const entry = { owner: null, basis: null, proposal: false, schema_js_definers: js, json_definers: json, in_contract: inContract };
    const base = baseline[name] === 'shell' ? PLATFORM_OWNER : baseline[name];
    if (OWNER_RESOLUTIONS[name] && known(OWNER_RESOLUTIONS[name].owner)) {
      Object.assign(entry, { owner: OWNER_RESOLUTIONS[name].owner, basis: `Entscheidung: ${OWNER_RESOLUTIONS[name].basis}` });
    } else if (base && known(base)) {
      Object.assign(entry, { owner: base, basis: 'Basis-Zuordnung ownership-baseline.json (Strang M, 8a332b929)' });
    } else if (json.length === 1) {
      Object.assign(entry, { owner: json[0], basis: 'neue Collection, nur in collections.schema.json dieser App definiert' });
    } else if (json.length > 1) {
      const pick = prefixOwner(name, json) || json[0];
      Object.assign(entry, { owner: pick, basis: `neue Collection, mehrere Definierer in collections.schema.json (${json.join(', ')}); Wahl per Präfixregel`, proposal: true });
    } else {
      const pick = prefixOwner(name);
      Object.assign(entry, pick && apps[pick]
        ? { owner: pick, basis: 'keine App definiert die Collection; Eigentümer per Präfixregel', proposal: true, no_defining_app: true }
        : { owner: PLATFORM_OWNER, basis: 'keine App definiert die Collection; keine Präfixregel → Plattform', proposal: true, no_defining_app: true });
    }
    ownership[name] = entry;
  }

  // ---- Nutzung je App
  const usage = {};
  const appTodoSeed = [];
  for (const app of Object.values(apps)) {
    const u = {};
    const add = (name, source) => {
      u[name] ||= { sources: new Set(), read: [], write: [], doc_ops: [] };
      u[name].sources.add(source);
    };
    for (const name of app.moduleJson.collections || []) add(name, 'module.json.collections');
    for (const name of Object.keys(app.jsonSchema.collections)) add(name, 'collections.schema.json');
    const hits = scanCode(app.dir, app.files, universe);
    for (const [name, h] of Object.entries(hits)) {
      add(name, 'code');
      u[name].read.push(...h.read);
      u[name].write.push(...h.write);
    }
    const decl = app.decl;
    for (const [name, ops] of Object.entries(decl.foreign_collections || {})) {
      add(name, DECLARATION_FILE);
      u[name].doc_ops.push(...ops);
    }
    // foreign_exclusive: nur Code-Belege und Deklaration zählen; foreign_drop: benannte Einträge ohne Code-Beleg entfallen.
    // foreign_drop schlägt den Zeilen-Scan (reine Lese-Literale sind oft Texte), nicht aber einen Schreibaufruf.
    for (const [name, x] of Object.entries(u)) {
      if (ownership[name]?.owner === app.id || x.sources.has(DECLARATION_FILE)) continue;
      if ((decl.foreign_drop || []).includes(name) && !x.write.length) { delete u[name]; continue; }
      if (decl.foreign_exclusive && !x.sources.has('code')) delete u[name];
    }
    for (const name of decl.foreign_drop || []) {
      if (u[name]?.write.length) appTodoSeed.push([app.id, `foreign_drop ${name}: ${DECLARATION_FILE} nimmt den Eintrag heraus, der Code schreibt die Collection aber (${u[name].write.slice(0, 2).join(', ')}) – Code-Beleg prüfen.`]);
    }
    for (const t of app.triggers) {
      if (t.on) { add(t.on.collection, 'trigger'); u[t.on.collection].doc_ops.push('read', 'watch'); }
    }
    usage[app.id] = u;
  }

  // ---- Fremd-Collections und Exporte
  const foreign = {};
  const exportsBy = {};
  const platformClamps = [];
  const appTodos = {};
  for (const app of Object.values(apps)) {
    foreign[app.id] = {};
    appTodos[app.id] = appTodoSeed.filter(([id]) => id === app.id).map(([, t]) => t);
    const declare = (name, owner, ops, evidence) => {
      const prev = foreign[app.id][name];
      if (prev) {
        prev.ops = sortOps([...prev.ops, ...ops]);
        prev._evidence.sources = sortedUnique([...prev._evidence.sources, ...evidence.sources]);
        if (evidence.replaces) prev._evidence.replaces = sortedUnique([...(prev._evidence.replaces || []), ...evidence.replaces]);
        return prev;
      }
      foreign[app.id][name] = { owner_app: owner, ops: sortOps(ops), _evidence: evidence };
      return foreign[app.id][name];
    };
    for (const [name, u] of Object.entries(usage[app.id]).sort(([a], [b]) => a.localeCompare(b))) {
      const owner = ownership[name]?.owner;
      if (!owner || owner === app.id) continue;
      let ops = sortOps([...(u.write.length ? ['read', 'write'] : ['read']), ...u.doc_ops]);
      if (ops.includes('watch') && !ops.includes('read')) ops = sortOps(['read', ...ops]);
      const evidence = {
        sources: [...u.sources].sort(),
        code_read: u.read.slice(0, 3),
        code_write: u.write.slice(0, 3),
        ...(u.sources.has(DECLARATION_FILE) && app.decl._basis?.foreign_collections ? { declaration_basis: app.decl._basis.foreign_collections } : {}),
      };
      const onlyCode = [...u.sources].every((s) => s === 'code');
      if (owner === PLATFORM_OWNER) {
        const targets = PLATFORM_REPLACEMENTS[name] || [name];
        for (const target of targets) {
          const exported = platform.exports[target];
          if (!exported) {
            platformClamps.push({ app: app.id, collection: target, wanted: ops, granted: [], reason: 'nicht in Plattform-exports' });
            appTodos[app.id].push(`${target}: Plattform exportiert die Collection nicht – Zugriff (${ops.join(',')}) nicht deklariert.`);
            continue;
          }
          const wanted = PLATFORM_REPLACEMENTS[name] ? ['read'] : ops;
          const granted = wanted.filter((op) => exported.includes(op));
          const dropped = wanted.filter((op) => !exported.includes(op));
          if (dropped.length) platformClamps.push({ app: app.id, collection: target, wanted, granted, reason: 'auf Plattform-exports beschränkt' });
          if (!granted.length) continue;
          const f = declare(target, PLATFORM_OWNER, granted, { ...evidence, ...(PLATFORM_REPLACEMENTS[name] ? { replaces: [name] } : {}) });
          if (dropped.length) f._ops_clamped = sortOps([...(f._ops_clamped || []), ...dropped]);
        }
        continue;
      }
      const f = declare(name, owner, ops, evidence);
      if (onlyCode && !u.doc_ops.length) f._proposal = 'nur per Code-Literal erkannt (Zeilen-Scan), Nutzung bestätigen';
      if (ownership[name].proposal) f._owner_proposal = ownership[name].basis;
      exportsBy[owner] ||= {};
      exportsBy[owner][name] = sortOps([...(exportsBy[owner][name] || []), ...ops]);
    }
  }

  // Fremdzugriffe der Shell (SHELL_FOREIGN) geben die Eigentümer-App frei wie die Nutzung durch andere Apps.
  const shellErrors = [];
  for (const [name, f] of Object.entries(SHELL_FOREIGN).sort(([a], [b]) => a.localeCompare(b))) {
    if (f.owner_app === PLATFORM_OWNER) continue;
    if (!ownership[name]) continue; // Mini-Repos ohne die Collection
    if (ownership[name].owner !== f.owner_app) {
      shellErrors.push({ app: SHELL_APP_ID, kind: 'shell_foreign_owner', detail: `${name}: SHELL_FOREIGN nennt ${f.owner_app}, Eigentümer ist ${ownership[name].owner}` });
      continue;
    }
    exportsBy[f.owner_app] ||= {};
    exportsBy[f.owner_app][name] = sortOps([...(exportsBy[f.owner_app][name] || []), ...f.ops]);
  }

  // Freigaben der Eigentümer-App (§16.34 (a), §16.36): deklarierte exports gelten zusätzlich zur Nutzung durch andere Apps.
  // Nur eigene Collections; Nutzung über die Deklaration hinaus wird gemeldet (exports_beyond_declaration).
  const exportsBeyondDeclaration = [];
  for (const app of Object.values(apps)) {
    const declared = app.decl.exports || {};
    for (const [name, ops] of Object.entries(declared).sort(([a], [b]) => a.localeCompare(b))) {
      if (ownership[name]?.owner !== app.id) { appTodos[app.id].push(`exports.${name}: ${DECLARATION_FILE} gibt eine Collection frei, die der App nicht gehört (Eigentümer ${ownership[name]?.owner ?? 'unbekannt'}).`); continue; }
      exportsBy[app.id] ||= {};
      exportsBy[app.id][name] = sortOps([...(exportsBy[app.id][name] || []), ...ops, ...(ops.includes('watch') || ops.includes('write') ? ['read'] : [])]);
    }
    for (const [name, ops] of Object.entries(exportsBy[app.id] || {})) {
      if (!declared[name]) continue;
      const beyond = ops.filter((op) => !declared[name].includes(op) && !(op === 'read' && declared[name].length));
      if (beyond.length) exportsBeyondDeclaration.push({ owner: app.id, collection: name, declared: declared[name], used: ops, beyond });
    }
  }

  // ---- Drift (Schema-Quellen)
  const drift = { schema_js_vs_json: [], migration_steps: [], schema_js_load_errors: [], contract_only: [], no_owner_app: [], multiple_owners: [], copy_divergence: [], prefix_mismatch: [], schema_missing: [], handler_conflicts: [], platform_clamps: platformClamps, exports_beyond_declaration: exportsBeyondDeclaration };
  for (const app of Object.values(apps)) {
    if (app.schemaJsError) drift.schema_js_load_errors.push({ app: app.id, error: app.schemaJsError });
    const js = app.schemaJs?.collections;
    const json = app.jsonSchema.collections;
    if (js) {
      for (const name of sortedUnique([...Object.keys(js), ...Object.keys(json)])) {
        if (!(name in js)) { drift.schema_js_vs_json.push({ app: app.id, collection: name, kind: 'only_in_collections.schema.json' }); continue; }
        if (!(name in json)) { drift.schema_js_vs_json.push({ app: app.id, collection: name, kind: 'only_in_schema.js' }); continue; }
        const diffs = cmpSchemas(js[name], json[name]);
        if (diffs.length) drift.schema_js_vs_json.push({ app: app.id, collection: name, kind: 'fields_differ', diffs });
      }
      for (const name of sortedUnique([...Object.keys(app.schemaJs.migrationSteps), ...Object.keys(app.jsonSchema.migrations)])) {
        const a = app.schemaJs.migrationSteps[name] || [];
        const b = migrationSteps(app.jsonSchema.migrations[name]);
        if (JSON.stringify(a) !== JSON.stringify(b)) drift.migration_steps.push({ app: app.id, collection: name, schema_js_steps: a, json_steps: b });
      }
    }
  }
  for (const [name, o] of Object.entries(ownership)) {
    if (o.in_contract && !o.json_definers.length) drift.contract_only.push({ collection: name, proposed_owner: o.owner });
    if (o.no_defining_app) drift.no_owner_app.push({ collection: name, proposed_owner: o.owner, basis: o.basis });
    if (o.json_definers.length > 1) drift.multiple_owners.push({ collection: name, definers: o.json_definers, owner: o.owner });
    const variants = new Map();
    for (const app of Object.values(apps)) {
      const s = app.jsonSchema.collections[name];
      if (!s) continue;
      const key = JSON.stringify(canon(s));
      if (!variants.has(key)) variants.set(key, { schema: s, sources: [] });
      variants.get(key).sources.push(`${app.id}/${app.schemaSource?.startsWith('v4/') ? app.schemaSource : 'collections.schema.json'}`);
    }
    if (variants.size > 1) {
      const ownerSchema = apps[o.owner]?.jsonSchema.collections[name];
      const ref = ownerSchema || [...variants.values()][0].schema;
      drift.copy_divergence.push({
        collection: name,
        owner: o.owner,
        reference: ownerSchema ? `${o.owner} (Eigentümer)` : [...variants.values()][0].sources[0],
        variants: [...variants.values()].map((v) => ({ sources: v.sources.sort(), diffs_vs_reference: cmpSchemas(v.schema, ref) }))
          .sort((x, y) => x.sources[0].localeCompare(y.sources[0])),
      });
    }
    const pref = prefixOwner(name);
    if (pref && apps[pref] && apps[o.owner] && pref !== o.owner) drift.prefix_mismatch.push({ collection: name, owner: o.owner, prefix_suggests: pref, basis: o.basis });
  }
  const handlerOwners = {};
  for (const app of Object.values(apps)) for (const type of Object.keys(app.handlers)) (handlerOwners[type] ||= []).push(app.id);
  for (const [type, owners] of Object.entries(handlerOwners).sort(([a], [b]) => a.localeCompare(b))) {
    if (owners.length > 1) drift.handler_conflicts.push({ command_type: type, apps: owners.sort() });
  }

  // ---- Manifeste
  const manifests = {};
  for (const app of Object.values(apps)) {
    const mj = app.moduleJson;
    const extras = app.decl;
    const basisOf = (field) => (extras._basis?.[field] ? `${DECLARATION_FILE}; ${extras._basis[field]}` : DECLARATION_FILE);
    const todo = [...appTodos[app.id]];
    const collections = {};
    const migrations = {};
    for (const name of universe.filter((n) => ownership[n].owner === app.id)) {
      const o = ownership[name];
      let schema = null;
      let schemaSource = null;
      const colTodo = [];
      if (app.jsonSchema.collections[name]) { schema = app.jsonSchema.collections[name]; schemaSource = app.schemaSource; }
      else if (o.json_definers[0]) {
        const other = apps[o.json_definers[0]];
        schema = other.jsonSchema.collections[name]; schemaSource = other.schemaSource;
        colTodo.push(`Schema nur in ${other.schemaSource}; in die collections.schema.json der Eigentümer-App übernehmen (§16.13).`);
      } else if (contract[name]) {
        schema = contract[name]; schemaSource = 'src/core/business_os/business_os_schema_contract.json';
        colTodo.push('Schema fehlt in collections.schema.json der Eigentümer-App; aus Schema-Vertrag übernommen – beim Port eintragen (§16.13).');
      } else {
        colTodo.push('Kein Schema in irgendeiner Quelle (keine collections.schema.json, kein Schema-Vertrag) – Schema-Strang (v4/schemas) muss es liefern.');
      }
      let klass = 'full';
      let classProposal = null;
      if (/_chunks$/.test(name)) { klass = 'blob'; classProposal = 'Chunk-Collection; §10.2a Nr. 4: Chunk-Collections entfallen, Inhalt über files/Spur 4 – Klasse blob bis zur Ablösung'; }
      else if (demandOnly[name]) { klass = 'window'; classProposal = `heute demand-only (${demandOnly[name]})`; }
      else if (PROJECTION_EVIDENCE[name]) { klass = 'projection'; classProposal = PROJECTION_EVIDENCE[name]; }
      if (extras.class_proposals?.[name]) classProposal = extras.class_proposals[name];
      const wrapper = app.jsonSchema.wrappers[name];
      const override = extras.collection_overrides?.[name] || {};
      collections[name] = {
        schema,
        class: klass,
        client_limit: null,
        byte_budget: null,
        rights: extras.collection_rights?.[name] ? { ...extras.collection_rights[name], _basis: basisOf('collection_rights') } : null,
        _schema_source: schemaSource,
        _owner_basis: o.basis,
        ...(o.proposal ? { _owner_proposal: true } : {}),
        ...(classProposal ? { _class_proposal: classProposal } : {}),
        ...(wrapper ? { _schema_wrapper: wrapper } : {}),
        ...(colTodo.length ? { _todo: colTodo } : {}),
        ...override,
      };
      const mig = app.jsonSchema.collections[name] ? app.jsonSchema.migrations[name] : null;
      if (mig?.kind === 'strategies') {
        const steps = migrationSteps(mig);
        migrations[name] = {
          from_ver: steps[0] - 1,
          steps: steps.map((v) => ({ to_ver: v, ...mig.value[String(v)] })),
          migrate_fn: null,
          _source: `${app.schemaSource} migration_strategies`,
        };
      } else if (mig?.kind === 'a9') {
        migrations[name] = { migrate_fn: null, ...mig.value, _source: `${app.schemaSource} migrations` };
      }
    }
    const indexJs = path.join(app.dir, 'index.js');
    const hasMount = fs.existsSync(indexJs) && /export\s+(async\s+)?function\s+mount\b/.test(fs.readFileSync(indexJs, 'utf8'));
    if (!hasMount) todo.push('index.js exportiert kein mount(); Einstieg klären.');
    const layoutSrc = mj.layout || {};
    const layout = {};
    // frame_palette wird nicht übernommen (M-17): die Kanten berechnet der Rahmen aus den Icon-Pixeln; Wächter
    // v4/tools/guards/frame-palette-rule.mjs meldet das Feld in module.json/v4-manifest.json.
    for (const key of ['shell', 'icon_asset', 'icon_asset_srcset', 'icon_asset_60', 'icon_svg', 'category', 'default_width', 'default_height', 'min_width', 'min_height', 'drawers']) {
      if (key in layoutSrc) layout[key] = layoutSrc[key];
    }
    const iconExternal = normalizeIconAssets(layout, app.rel, app.files);
    // MAN2 (M-09): gewähltes Raster-Icon (Shell-V2-Referenz) als Paketpfad; ersetzt fremde icon_asset*-Pfade.
    const iconSel = iconSelection[app.id];
    const iconTarget = iconSel ? packageIconTarget(iconSel, app.rel) : null;
    // MAN3: abgeleitete Paket-Icons (256 px + @512) gelten, wenn sie zum Ableitungsbuch passen; eigenes Paketbild wie MAN2.
    const iconDeriv = iconSync.derivations.apps[app.id];
    const iconDerivedOk = !!(iconSel && iconTarget.startsWith(`${ICON_PACKAGE_BASENAME}.`) && iconDeriv && iconDeriv.source_sha256 === iconSel.sha256
      && Object.entries(iconDeriv.outputs || {}).every(([rel, o]) => app.files[rel] === o.sha256) && iconDeriv.outputs?.[iconTarget]);
    if (iconSel && (iconDerivedOk || app.files[iconTarget] === iconSel.sha256)) {
      if (layout.icon_asset !== iconTarget || iconDerivedOk) {
        delete layout.icon_asset_srcset;
        delete layout.icon_asset_60;
      }
      layout.icon_asset = iconTarget;
      if (iconDerivedOk) {
        const outs = Object.entries(iconDeriv.outputs).map(([rel, o]) => ({ rel, px: o.px })).sort((a, b) => a.px - b.px);
        const srcset = iconSrcsetFor(outs);
        if (srcset) layout.icon_asset_srcset = srcset;
        layout._icon_asset_source = `${iconSel.asset} (${iconSel.source}, sha256 ${iconSel.sha256.slice(0, 12)}…, ${iconDeriv.source_px} px; Paket: ${outs.map((o) => `${o.px} px`).join(' + ')} laut ${ICON_DERIVATIONS_FILE}; Shell-V2 zeigt dasselbe Bild)`;
      } else {
        layout._icon_asset_source = `${iconSel.asset} (${iconSel.source}, sha256 ${iconSel.sha256.slice(0, 12)}…; Shell-V2 zeigt dasselbe Bild)`;
      }
      if (Object.keys(iconExternal).length) layout._icon_asset_replaced = { ...iconExternal };
      for (const key of Object.keys(iconExternal)) delete iconExternal[key];
    }
    if (Object.keys(iconExternal).length) {
      layout._icon_asset_external = iconExternal;
      todo.push(`layout.${Object.keys(iconExternal).join('/')} liegt nicht im App-Paket (${[...new Set(Object.values(iconExternal))].join(', ')}): Datei ins Paket kopieren (z. B. assets/icon/) und module.json auf den Paketpfad umstellen (§16.39 Nr. 6). Bis dahin zeigt die Shell icon_svg.`);
    }
    if (!('category' in layout) && mj.category) layout.category = mj.category;
    if (mj.presentation) layout.presentation = mj.presentation;
    // multi_instance (v4/frame/INTEGRATION.md FR2-4): Deklaration vor module.json presentation.multi_instance. Der Rahmen
    // liest heute layout.presentation.multi_instance (v4/frame/index.js) – beide Stellen tragen denselben Wert.
    if (typeof extras.multi_instance === 'boolean') {
      layout.multi_instance = extras.multi_instance;
      layout.presentation = { ...(layout.presentation || {}), multi_instance: extras.multi_instance };
      layout._multi_instance_basis = basisOf('multi_instance');
    } else if (typeof mj.presentation?.multi_instance === 'boolean') {
      layout.multi_instance = mj.presentation.multi_instance;
    }
    // K-WJ-D1: Mobil-Eintrag der App (V2-Form ohne id) unter layout.presentation.mobile, siehe MOBILE_APPS.
    if (MOBILE_APPS.has(app.id)) layout.presentation = { ...(layout.presentation || {}), mobile: MOBILE_APPS.get(app.id) };
    if (mj.icon) layout.icon = mj.icon;
    if (app.files['icon.svg']) layout._icon_file = 'icon.svg';
    if (extras.layout_icon_svg_from && !layout.icon_svg && fs.existsSync(path.join(app.dir, extras.layout_icon_svg_from))) {
      layout.icon_svg = fs.readFileSync(path.join(app.dir, extras.layout_icon_svg_from), 'utf8').trim();
      layout._icon_svg_source = `${extras.layout_icon_svg_from} (module.json hat kein icon_svg; Owner-Auflage 15.10.: Logo muss erhalten bleiben)`;
    }
    // Inline-SVG als <img src="data:…"> braucht den SVG-Namensraum, sonst zeigt der Browser ein kaputtes Bild.
    if (typeof layout.icon_svg === 'string' && /^<svg\b/i.test(layout.icon_svg) && !/\bxmlns\s*=/.test(layout.icon_svg)) {
      layout.icon_svg = layout.icon_svg.replace(/^<svg\b/i, '<svg xmlns="http://www.w3.org/2000/svg"');
    }
    // styles (§16.19 Nr. 6): die Shell lädt index.css; weitere CSS lädt die App selbst (@import bzw. Laufzeit-Link).
    const styles = deriveStyles(app.files);
    for (const extra of extras.styles || []) {
      if (!app.files[extra]) { todo.push(`styles ${extra} (${DECLARATION_FILE}): Datei fehlt im App-Paket.`); continue; }
      if (!styles.includes(extra)) styles.push(extra);
    }
    if (hasMount && styles.includes('index.css') && /['"`]\.\/index\.css['"`]/.test(fs.readFileSync(indexJs, 'utf8'))) {
      todo.push('index.js lädt index.css noch selbst; Laden übernimmt die Shell über styles (§16.19 Nr. 6) – Selbstladen beim Port entfernen.');
    }

    // Handler, Trigger, Rechte, Capabilities, Legacy
    const handlers = app.handlers;
    const triggers = [];
    for (const t of app.triggers) {
      if (!Object.values(handlers).includes(t.handler)) { todo.push(`Trigger-Handler ${t.handler} fehlt unter handlers/ (${t._source}).`); continue; }
      if (t.on && ownership[t.on.collection]?.owner !== app.id && !foreign[app.id][t.on.collection]?.ops.includes('watch')) {
        todo.push(`Trigger ${t.handler} auf ${t.on.collection}: weder eigene Collection noch foreign mit watch (§16.34).`);
        continue;
      }
      const { _source: source, ...plain } = t;
      triggers.push({ ...plain, ...(source === DECLARATION_FILE ? {} : { _source: source }) });
    }
    triggers.sort((a, b) => a.handler.localeCompare(b.handler) || JSON.stringify(a.on ?? a.every).localeCompare(JSON.stringify(b.on ?? b.every)));
    const commandRights = { ...app.code.rights };
    // net.ingest (§16.38/§16.39): der Zustell-Befehlstyp ist Handler der App; Zustellung nur durch den Core, nie per UI.
    const netIngest = app.netIngest.map((n) => ({ ...n }));
    for (const n of netIngest) {
      if (!n.command) { todo.push(`net.ingest-Anmeldung ${n._source}: command nicht auflösbar (Literal/Konstante erwartet).`); continue; }
      if (!(n.command in handlers)) todo.push(`net.ingest command ${n.command} (${n._source}) hat keinen Handler – der Core lehnt register mit invalid ab (§16.39).`);
      else if (!commandRights[n.command]) commandRights[n.command] = { roles: [], assigned_users: false, _basis: `§16.38/§16.39: Zustellung nur durch net.ingest (${n._source}), kein UI-Aufruf` };
    }
    for (const [key, r] of Object.entries(extras.command_rights || {})) {
      const matches = key.endsWith('*') ? Object.keys(handlers).some((t) => t.startsWith(key.slice(0, -1))) : key in handlers;
      if (!matches) { todo.push(`command_rights ${key}: kein passender Handler.`); continue; }
      commandRights[key] = { ...r, _basis: basisOf('command_rights') };
    }
    const legacyTypes = Object.keys(app.code.legacy);
    const capNames = Object.keys(app.code.capabilities);
    const capabilitiesRequired = sortedUnique([...capNames, ...legacyTypes.map((t) => `legacy.${t}`)]);
    for (const [rule, ruleBasis] of Object.entries(app.code.rights)) if (!(rule in handlers)) todo.push(`command_rights ${rule} (${ruleBasis._permission}): kein Handler-Datei.`);
    if (legacyTypes.length) todo.push(`${legacyTypes.length} Befehlstyp(en) laufen noch über ctx.cap.legacy (Ziel 0): ${legacyTypes.join(', ')}.`);
    for (const c of drift.handler_conflicts.filter((x) => x.apps.includes(app.id))) {
      todo.push(`Befehlstyp ${c.command_type} hat Handler in mehreren Apps (${c.apps.join(', ')}); der Core muss eindeutig routen.`);
    }
    for (const v of app.agent.violations) {
      todo.push(v.ref
        ? `Skill-Verweis ${v.ref} (${v.file}:${v.line}): ${v.kind === 'foreign_namespace' ? 'fremder Namensraum' : 'nicht unter skills/ deklariert'} (§16.30).`
        : `${v.path}: ${v.kind} (§16.30 Paketdateien skills/<name>/SKILL.md, prompts/<name>.md).`);
    }
    if (extras._proposed_triggers) todo.push(`Projektions-Auslöser (W-4) noch nicht im Vertrag: siehe _proposed_triggers.`);
    for (const extra of extras.todo || []) todo.push(extra);
    todo.push('rights je Collection: module.json hat kein permissions-Feld → null; Rechte aus policy.rs/permissions.js beim Port festlegen.');
    todo.push('client_limit/byte_budget: keine Ist-Messung im Repo → null.');
    todo.push('app_ver/author/created_at setzt der Core bei der Veröffentlichung (§9.1, §9.2).');
    if (app.files['schema.js']) todo.push('schema.js ist noch Teil der Dateien; entfällt beim Port (§16.13).');
    if (app.schemaJsError) todo.push(`schema.js lässt sich nicht laden (${app.schemaJsError}); nur für den Drift-Bericht relevant, beim Port löschen.`);
    for (const name of Object.keys(collections).filter((n) => !collections[n].schema)) drift.schema_missing.push({ app: app.id, collection: name });

    const rust = analysis[app.id]?.rust;
    manifests[app.id] = {
      _format: MANIFEST_FORMAT,
      _generator: GENERATOR_ID,
      _source_dir: `src/apps/business-os/${app.rel}`,
      app_id: app.id,
      title: mj.title ?? null,
      version: mj.version ?? null,
      app_ver: null,
      author: null,
      created_at: null,
      entry: hasMount ? 'index.js' : null,
      _legacy_entry: mj.entry ?? null,
      files: app.files,
      collections,
      migrations,
      foreign_collections: foreign[app.id],
      exports: exportsBy[app.id] || {},
      handlers,
      triggers,
      command_rights: commandRights,
      capabilities_required: capabilitiesRequired,
      _capability_evidence: app.code.capabilities,
      legacy: legacyTypes,
      _legacy_count: legacyTypes.length,
      // §16.30: nur bei vorhandenen Paketdateien ausgegeben; fehlendes Feld = [] (keine Änderung bestehender Manifeste).
      ...(app.agent.skills.length ? { skills: app.agent.skills } : {}),
      ...(app.agent.prompts.length ? { prompts: app.agent.prompts } : {}),
      ...(extras.row_filters ? { row_filters: extras.row_filters } : {}),
      ...(extras.write_guard ? { write_guard: extras.write_guard } : {}),
      ...(extras._proposed_triggers ? { _proposed_triggers: extras._proposed_triggers } : {}),
      commands_out: [],
      commands_in: (extras.commands_in || []).map((c) => ({ type: c.type, from: c.from === '*' ? '*' : sortedUnique(c.from) }))
        .sort((a, b) => a.type.localeCompare(b.type)),
      ...(netIngest.length ? { _net_ingest: netIngest } : {}),
      ...(app.configKeys.length ? { _config_keys: app.configKeys } : {}),
      ...(app.declPresent ? { _declaration: `${app.rel}/${DECLARATION_FILE}` } : {}),
      ...(rust ? { _rust_basis: rust } : {}),
      min_app_api: MIN_APP_API,
      provenance: { base_release: baseRelease, base_hash: baseHash(app.files), locally_modified: false },
      layout,
      styles,
      ...(styles.length ? { _styles_basis: `§16.19 Nr. 6; index.css der App${extras.styles?.length ? ` + ${basisOf('styles')}` : ''} (Shell lädt sie, Reihenfolge = Ladereihenfolge)` } : {}),
      _todo: todo,
    };
  }

  // ---- Shell-Manifest
  manifests[SHELL_APP_ID] = buildShellManifest(appsRoot, baseRelease, platform);
  // SHELL_FOREIGN auf App-Collections, die es in diesem Baum nicht gibt (Mini-Repos), entfallen mit _todo.
  for (const [name, f] of Object.entries(SHELL_FOREIGN)) {
    if (f.owner_app === PLATFORM_OWNER || ownership[name]) continue;
    delete manifests[SHELL_APP_ID].foreign_collections[name];
    manifests[SHELL_APP_ID]._todo.push(`foreign ${name}: Collection fehlt im Baum (Eigentümer ${f.owner_app}).`);
  }
  // SHELL_COMMANDS_OUT an Apps, die es in diesem Baum nicht gibt (Mini-Repos), entfallen ebenso mit _todo.
  for (const c of SHELL_COMMANDS_OUT) {
    if (apps[c.app]) continue;
    const sm = manifests[SHELL_APP_ID];
    sm.commands_out = sm.commands_out.filter((x) => !(x.app === c.app && x.type === c.type));
    delete sm._commands_out_evidence[`${c.app} ${c.type}`];
    sm._todo.push(`commands_out ${c.app} ${c.type}: Ziel-App fehlt im Baum.`);
  }
  const libs = buildLibManifests(appsRoot, baseRelease);

  // ---- Prüfungen
  const exportsOf = (owner) => (owner === PLATFORM_OWNER ? platform.exports : manifests[owner]?.exports);
  const foreignWithoutExport = [];
  for (const [id, m] of Object.entries(manifests)) {
    for (const [name, f] of Object.entries(m.foreign_collections)) {
      const ex = exportsOf(f.owner_app)?.[name];
      const missing = f.ops.filter((op) => !ex?.includes(op));
      if (!ex || missing.length) foreignWithoutExport.push({ app: id, collection: name, owner: f.owner_app, missing: ex ? missing : f.ops });
    }
  }
  drift.foreign_without_export = foreignWithoutExport;

  // ---- App-übergreifende Befehle (§16.40): commands_out aus dem Code (+ Deklaration für dynamische Ziele),
  // commands_in aus der Deklaration der Empfänger-App. Jede Seite ohne Gegenstück ist ein Befund (Soll 0), weil der
  // Core den Befehl sonst mit forbidden abweist.
  drift.commands_unmatched = [];
  drift.commands_out_unresolved = [];
  drift.commands_in_without_handler = [];
  // ROUTE (E39/E40): jeder Typ, den eine App (oder die Shell) über api.command sendet, ist Plattformroute ODER geht per
  // commands_out an genau die App mit dem Handler (deren commands_in nimmt den konkreten Absender an, nie '*').
  // Sonst drift.commands_unrouted (rot), außer der Typ steht in PLATFORM_ROUTES_PENDING (gemeldet, Bericht).
  const routes = loadPlatformRoutes(repo);
  const uiRoute = routeUiCommands({
    senders: [
      ...Object.values(apps).map((a) => ({ id: a.id, rel: a.rel, hits: a.uiCommandTypes })),
      { id: SHELL_APP_ID, rel: '', hits: scanUiCommandTypes(appsRoot, shellUiFiles(manifests[SHELL_APP_ID].files)) },
    ],
    handlerTypesByApp,
    routes,
  });
  drift.commands_out_ambiguous = uiRoute.ambiguous;
  drift.commands_unrouted = uiRoute.unrouted;
  drift.commands_routes_pending = uiRoute.pending;
  drift.commands_in_derived = [];
  const uiOutBy = {};
  for (const o of uiRoute.out) (uiOutBy[o.from] ||= []).push(o);
  for (const o of uiRoute.out.filter((x) => x.from === SHELL_APP_ID)) {
    const sm = manifests[SHELL_APP_ID];
    if (!sm.commands_out.some((c) => c.app === o.app && c.type === o.type)) sm.commands_out.push({ app: o.app, type: o.type });
    sm.commands_out.sort((a, b) => a.app.localeCompare(b.app) || a.type.localeCompare(b.type));
    const key = `${o.app} ${o.type}`;
    sm._commands_out_evidence[key] = sortedUnique([...(sm._commands_out_evidence[key] || []), o.evidence]);
  }
  for (const app of Object.values(apps)) {
    const m = manifests[app.id];
    const evidence = {};
    const addOut = (target, type, ev) => { (evidence[`${target} ${type}`] ||= new Set()).add(ev); };
    for (const c of app.commandsOutCode) addOut(c.app, c.type, c.evidence);
    for (const c of uiOutBy[app.id] || []) addOut(c.app, c.type, c.evidence);
    for (const c of app.decl.commands_out || []) addOut(c.app, c.type, `${DECLARATION_FILE}${app.decl._basis?.commands_out ? `; ${app.decl._basis.commands_out}` : ''}`);
    m.commands_out = Object.keys(evidence).sort().map((k) => { const [target, type] = k.split(' '); return { app: target, type }; });
    if (m.commands_out.length) m._commands_out_evidence = Object.fromEntries(Object.entries(evidence).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, [...v].sort()]));
    const covered = new Set((app.decl.commands_out || []).map((c) => c.app));
    const unresolved = app.commandsOutUnresolved.filter((u) => !(app.decl.commands_out || []).some((c) => c.from_file === u.file) && !covered.has('*'));
    if (unresolved.length) {
      m._commands_out_unresolved = unresolved;
      for (const u of unresolved) drift.commands_out_unresolved.push({ app: app.id, ...u });
      m._todo.push(`${unresolved.length} ctx.command-Aufruf(e) mit nicht auflösbarem Ziel (${unresolved.map((u) => `${u.file}:${u.line}`).join(', ')}): Ziel-App/Typ zur Laufzeit bestimmt – commands_out in ${DECLARATION_FILE} deklarieren oder Ziel festlegen (§16.40).`);
    }
    for (const c of m.commands_in) {
      if (!(c.type in m.handlers)) {
        drift.commands_in_without_handler.push({ app: app.id, type: c.type });
        m._todo.push(`commands_in ${c.type}: kein Handler unter handlers/.`);
      }
    }
  }
  // commands_in aus der gemessenen UI-Nutzung: je konkretem Absender, nie '*'; vorhandene '*'-Einträge decken ab.
  for (const o of uiRoute.out) {
    const tm = manifests[o.app];
    if (!tm) continue;
    const key = `${o.from} ${o.type}`;
    const prev = drift.commands_in_derived.find((d) => d.app === o.app && d.from === o.from && d.type === o.type);
    if (prev) { prev.evidence = sortedUnique([...prev.evidence, o.source]); }
    const entry = tm.commands_in.find((c) => c.type === o.type);
    if (entry && (entry.from === '*' || (entry.from.includes(o.from) && !tm._commands_in_evidence?.[key]))) continue;
    if (!prev) drift.commands_in_derived.push({ app: o.app, from: o.from, type: o.type, evidence: [o.source] });
    if (!entry) tm.commands_in.push({ type: o.type, from: [o.from] });
    else if (!entry.from.includes(o.from)) entry.from = sortedUnique([...entry.from, o.from]);
    tm.commands_in.sort((a, b) => a.type.localeCompare(b.type));
    tm._commands_in_evidence ||= {};
    tm._commands_in_evidence[key] = sortedUnique([...(tm._commands_in_evidence[key] || []), o.source]);
  }
  drift.commands_in_derived.sort((a, b) => a.app.localeCompare(b.app) || a.from.localeCompare(b.from) || a.type.localeCompare(b.type));
  for (const [id, m] of Object.entries(manifests)) {
    for (const c of m.commands_out || []) {
      const target = manifests[c.app];
      let reason = null;
      if (!target || c.app === SHELL_APP_ID) reason = 'Ziel-App unbekannt';
      else if (!(c.type in target.handlers)) reason = 'Ziel-App hat keinen Handler für den Typ';
      else if (!(target.commands_in || []).some((x) => x.type === c.type && (x.from === '*' || x.from.includes(id)))) reason = 'Ziel-App nimmt den Typ von dieser App nicht an (commands_in)';
      if (reason) {
        drift.commands_unmatched.push({ from: id, app: c.app, type: c.type, reason });
        m._todo.push(`commands_out ${c.app} ${c.type}: ${reason} (§16.40).`);
      }
    }
  }

  const platformOut = {
    [PLATFORM_OWNER]: {
      source: platform.source,
      collections: universe.filter((n) => ownership[n].owner === PLATFORM_OWNER),
      exports: platform.exports,
      used_by: Object.fromEntries(Object.keys(platform.exports).sort().map((n) => [n, Object.keys(manifests).filter((id) => manifests[id].foreign_collections[n]).sort()])),
    },
  };

  const appManifests = Object.values(manifests).filter((m) => m.app_id !== SHELL_APP_ID);
  const allCaps = sortedUnique(appManifests.flatMap((m) => m.capabilities_required.filter((c) => !c.startsWith('legacy.'))));
  const stats = {
    apps: appIds.length,
    manifests_total: Object.keys(manifests).length,
    collections_total: universe.length,
    collections_in_contract: Object.keys(contract).length,
    owned_by_apps: universe.filter((n) => apps[ownership[n].owner]).length,
    owned_by_platform: platformOut[PLATFORM_OWNER].collections.length,
    owner_proposals: universe.filter((n) => ownership[n].proposal).length,
    handlers_total: appManifests.reduce((s, m) => s + Object.keys(m.handlers).length, 0),
    apps_with_handlers: appManifests.filter((m) => Object.keys(m.handlers).length).length,
    legacy_total: appManifests.reduce((s, m) => s + m.legacy.length, 0),
    capabilities_distinct: allCaps.length,
    triggers_total: appManifests.reduce((s, m) => s + m.triggers.length, 0),
    command_rights_entries: appManifests.reduce((s, m) => s + Object.keys(m.command_rights).length, 0),
    foreign_declarations: Object.values(manifests).reduce((s, m) => s + Object.keys(m.foreign_collections).length, 0),
    foreign_write_declarations: Object.values(manifests).reduce((s, m) => s + Object.values(m.foreign_collections).filter((x) => x.ops.includes('write')).length, 0),
    foreign_code_only: Object.values(manifests).reduce((s, m) => s + Object.values(m.foreign_collections).filter((x) => x._proposal).length, 0),
    foreign_without_export: foreignWithoutExport.length,
    platform_clamps: platformClamps.length,
    exported_collections: Object.values(exportsBy).reduce((s, e) => s + Object.keys(e).length, 0),
    handler_conflicts: drift.handler_conflicts.length,
    schema_missing: drift.schema_missing.length,
    schema_js_present: Object.values(apps).filter((a) => a.files['schema.js']).length,
    schema_js_load_errors: drift.schema_js_load_errors.length,
    schema_from_v4_schemas: Object.values(apps).filter((a) => a.schemaSource?.startsWith('v4/')).length,
    no_owner_app: drift.no_owner_app.length,
    contract_only: drift.contract_only.length,
    multiple_owners: drift.multiple_owners.length,
    drift_schema_js_vs_json_collections: drift.schema_js_vs_json.length,
    drift_migration_steps: drift.migration_steps.length,
    drift_copy_divergence: drift.copy_divergence.length,
    prefix_mismatch: drift.prefix_mismatch.length,
    class_proposals: appManifests.reduce((s, m) => s + Object.values(m.collections).filter((c) => c.class !== 'full').length, 0),
    shell_files: Object.keys(manifests[SHELL_APP_ID].files).length,
    apps_with_styles: appManifests.filter((m) => m.styles.length).length,
    write_guards: appManifests.reduce((s, m) => s + Object.keys(m.write_guard || {}).length, 0),
    todo_total: Object.values(manifests).reduce((s, m) => s + m._todo.length, 0),
    declarations: Object.values(apps).filter((a) => a.declPresent).length,
    commands_out_total: appManifests.reduce((s, m) => s + m.commands_out.length, 0),
    commands_in_total: appManifests.reduce((s, m) => s + m.commands_in.length, 0),
    commands_unmatched: drift.commands_unmatched.length,
    commands_unrouted: drift.commands_unrouted.length,
    commands_out_ambiguous: drift.commands_out_ambiguous.length,
    commands_routes_pending: drift.commands_routes_pending.length,
    commands_in_derived: drift.commands_in_derived.length,
    commands_out_unresolved: drift.commands_out_unresolved.length,
    commands_in_without_handler: drift.commands_in_without_handler.length,
    exports_beyond_declaration: drift.exports_beyond_declaration.length,
    net_ingest_total: appManifests.reduce((s, m) => s + (m._net_ingest || []).length, 0),
    multi_instance_false: appManifests.filter((m) => m.layout.multi_instance === false).length,
    // MAN2 (M-09): Icon-Herkunft je App – Rasterbild im Paket, nur SVG, oder generiertes Ersatz-Icon des Rahmens (FR6a).
    icon_raster: appManifests.filter((m) => m.layout.icon_asset && !/\.svg$/i.test(m.layout.icon_asset)).length,
    icon_svg: appManifests.filter((m) => (m.layout.icon_asset && /\.svg$/i.test(m.layout.icon_asset)) || (!m.layout.icon_asset && m.layout.icon_svg)).length,
    icon_generated: appManifests.filter((m) => !m.layout.icon_asset && !m.layout.icon_svg).map((m) => m.app_id).sort(),
  };

  // §16.30-Prüfung (Skills/Prompts): nicht Teil von renderOutputs, damit bestehende Berichte unverändert bleiben;
  // main() endet bei Verstößen mit Exit 1.
  const skillCheck = { violations: Object.values(apps).flatMap((a) => a.agent.violations) };
  // Deklarationsdateien: Form-/Schlüsselfehler und nicht auswertbare Trigger-Exporte (main() endet mit Exit 1).
  const declarationCheck = { violations: [...Object.values(apps).flatMap((a) => a.declErrors), ...shellErrors, ...iconSync.problems] };
  // E40: nicht routbare und mehrdeutige UI-Befehle (main() endet mit Exit 1).
  const routeCheck = {
    violations: [
      ...drift.commands_unrouted.map((d) => ({ app: d.app, kind: 'commands_unrouted', detail: `${d.type} (${d.evidence.join(', ')})` })),
      ...drift.commands_out_ambiguous.map((d) => ({ app: d.app, kind: 'commands_out_ambiguous', detail: `${d.type} → ${d.handlers.join('|')} (${d.evidence.join(', ')})` })),
    ],
  };
  return { manifests, libs, ownership, platform: platformOut, drift, stats, capabilities: allCaps, skill_check: skillCheck, declaration_check: declarationCheck, route_check: routeCheck, icons_copied: iconSync.copied };
}

// ---------------------------------------------------------------- Shell-Manifest

// styles (§16.19 Nr. 6): index.css im App-Wurzelverzeichnis; die Apps laden weitere CSS selbst (documents/spreadsheets per
// @import ctox:shared/office-workspace.css, mail editor/editor.css aus mail-content-editor.mjs, Vendor-CSS unter ../../vendor).
// §16.39 Nr. 6: icon_asset* ist ein Pfad im App-Paket (Kernel/Core lösen nur Paketpfade auf). module.json nennt sie
// heute meist relativ zu business-os (modules/<app>/…, customer-modules/<app>/…, Altname local-modules/<app>/…) oder
// schon paketrelativ. Alles andere (shared/…, fremde App, URL, data:, Pfadflucht, Datei fehlt im Paket) ist kein
// Paketpfad → null; der Aufrufer meldet es als _todo und behält den Altwert in _icon_asset_external.
export const ICON_ASSET_KEYS = ['icon_asset', 'icon_asset_60', 'icon_asset_srcset'];
export function packageAssetPath(value, appRel, files) {
  if (typeof value !== 'string' || !value.trim()) return null;
  let p = value.trim();
  if (/^[a-z][a-z0-9+.-]*:/i.test(p) || p.startsWith('//')) return null;
  p = p.replace(/^(\.\/|\/)+/, '');
  const [root, dir] = String(appRel || '').split('/');
  const prefixes = [`${root}/${dir}/`];
  if (root === 'customer-modules') prefixes.push(`local-modules/${dir}/`);
  const own = prefixes.find((x) => p.startsWith(x));
  if (own) p = p.slice(own.length);
  p = path.posix.normalize(p);
  if (!p || p === '.' || p === '..' || p.startsWith('../') || p.startsWith('/')) return null;
  return files && Object.prototype.hasOwnProperty.call(files, p) ? p : null;
}
// srcset "a.png 1x, b.png 2x": jeder Eintrag einzeln; nur Paketpfade bleiben, fremde fallen weg (→ external).
export function packageSrcset(value, appRel, files) {
  if (typeof value !== 'string' || !value.trim()) return { value: null, external: [] };
  const kept = [];
  const external = [];
  for (const part of value.split(',').map((x) => x.trim()).filter(Boolean)) {
    const [url, ...desc] = part.split(/\s+/);
    const p = packageAssetPath(url, appRel, files);
    if (p) kept.push([p, ...desc].join(' ')); else external.push(url);
  }
  return { value: kept.length ? kept.join(', ') : null, external };
}
export function normalizeIconAssets(layout, appRel, files) {
  const external = {};
  for (const key of ICON_ASSET_KEYS) {
    if (!(key in layout)) continue;
    const raw = layout[key];
    if (key === 'icon_asset_srcset') {
      const r = packageSrcset(raw, appRel, files);
      layout[key] = r.value;
      if (r.external.length) external[key] = raw;
    } else {
      layout[key] = packageAssetPath(raw, appRel, files);
      if (layout[key] == null && raw != null && raw !== '') external[key] = raw;
    }
  }
  return external;
}

// MAN2: Auswahlprojektionen laden (ICON_SELECTION_SOURCES, erste Quelle gewinnt wie in app.js). Fehlt eine Datei
// (Mini-Repos), ist die Auswahl leer.
export async function loadIconSelection(appsRoot) {
  const out = {};
  for (const src of ICON_SELECTION_SOURCES) {
    const file = path.join(appsRoot, src.file);
    if (!fs.existsSync(file)) continue;
    const mod = await import(`${pathToFileURL(file).href}?sha=${sha256(fs.readFileSync(file)).slice(0, 16)}`);
    for (const [appId, sel] of Object.entries(mod[src.table] || {}).sort(([a], [b]) => a.localeCompare(b))) {
      if (out[appId] || typeof sel?.asset !== 'string' || typeof sel?.sha256 !== 'string') continue;
      out[appId] = { asset: sel.asset, sha256: sel.sha256, source: src.file };
    }
  }
  return out;
}

// Paketpfad des gewählten Bildes: liegt es schon im eigenen Paket, gilt dieser Pfad, sonst die Kopie assets/icon.<ext>.
export function packageIconTarget(sel, appRel) {
  const [root, dir] = String(appRel || '').split('/');
  const prefixes = [`${root}/${dir}/`, ...(root === 'customer-modules' ? [`local-modules/${dir}/`] : [])];
  const own = prefixes.find((p) => sel.asset.startsWith(p));
  if (own) return path.posix.normalize(sel.asset.slice(own.length));
  return `${ICON_PACKAGE_BASENAME}${path.posix.extname(sel.asset).toLowerCase()}`;
}

// MAN3: Die Paketkopie ist nicht mehr das 1024-px-Original, sondern je App ein 256-px-Bild (assets/icon.<ext>) plus
// assets/icon@512.<ext> für Retina (nur wenn die Quelle größer als 256 px ist). Ableitung im Schreiblauf mit ffmpeg
// (plattformneutral; Rückfall sips unter macOS), nie hochskaliert: ist die Quelle <= Zielgröße, wird byte-gleich kopiert.
// Die Bytes hängen vom Werkzeug ab; deshalb prüft --check nicht gegen Byte-Gleichheit mit der Quelle, sondern gegen das
// Ableitungsbuch ICON_DERIVATIONS_FILE: Quelle + Quell-sha256 (= Auswahl-sha256) + sha256 jeder abgeleiteten Datei.
// Weicht die Quelle, die Auswahl oder eine Paketdatei ab, meldet --check icon_package (Exit 1). --check braucht kein Werkzeug.
export const ICON_SIZES = [{ suffix: '', px: 256 }, { suffix: '@512', px: 512 }];
export const ICON_DERIVATIONS_FILE = 'v4/tools/manifest/icon-derivations.json';
export const ICON_DERIVATIONS_FORMAT = 'ctox-shell-v4-icon-derivations-1';

// Pixelmaße aus dem Dateikopf (PNG IHDR, JPEG SOFn), reines JS. Unbekannt → null.
export function imageSize(buf) {
  if (!Buffer.isBuffer(buf) || buf.length < 24) return null;
  if (buf.readUInt32BE(0) === 0x89504e47 && buf.toString('latin1', 12, 16) === 'IHDR') {
    return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
  }
  if (buf[0] === 0xff && buf[1] === 0xd8) {
    let i = 2;
    while (i + 9 < buf.length) {
      if (buf[i] !== 0xff) { i += 1; continue; }
      const marker = buf[i + 1];
      if (marker === 0xff) { i += 1; continue; }
      if (marker === 0xd8 || marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7)) { i += 2; continue; }
      const len = buf.readUInt16BE(i + 2);
      if (marker >= 0xc0 && marker <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(marker)) {
        return { height: buf.readUInt16BE(i + 5), width: buf.readUInt16BE(i + 7) };
      }
      i += 2 + len;
    }
  }
  return null;
}

// Zielpfade je Größe: assets/icon.jpg → [assets/icon.jpg 256, assets/icon@512.jpg 512]; @512 nur bei Quelle > 256 px.
export function iconOutputs(target, srcPx) {
  const ext = path.posix.extname(target);
  const base = target.slice(0, -ext.length);
  return ICON_SIZES.filter((s, i) => i === 0 || srcPx > ICON_SIZES[i - 1].px).map((s) => ({ rel: `${base}${s.suffix}${ext}`, px: s.px }));
}

// srcset aus den Paketdateien (1x = 256 px, 2x = 512 px); null, wenn es nur eine Größe gibt.
export function iconSrcsetFor(outputs) {
  return outputs.length > 1 ? outputs.map((o, i) => `${o.rel} ${i + 1}x`).join(', ') : null;
}

export function loadIconDerivations(appsRoot) {
  const file = path.join(appsRoot, ICON_DERIVATIONS_FILE);
  const data = fs.existsSync(file) ? readJson(file) : {};
  return { _format: ICON_DERIVATIONS_FORMAT, apps: data.apps && typeof data.apps === 'object' ? data.apps : {} };
}

function haveTool(cmd, args) {
  try { return spawnSync(cmd, args, { stdio: 'ignore' }).status === 0; } catch { return false; }
}

// Verkleinert srcAbs auf höchstens px × px (Seitenverhältnis bleibt). Werkzeug: ffmpeg, sonst sips (macOS). Liefert die
// Werkzeugkennung fürs Ableitungsbuch; ohne Werkzeug Fehler (Schreiblauf meldet icon_package).
export function resizeIcon(srcAbs, dstAbs, px) {
  const jpg = /\.jpe?g$/i.test(dstAbs);
  if (haveTool('ffmpeg', ['-version'])) {
    const args = ['-v', 'error', '-y', '-i', srcAbs, '-vf', `scale=${px}:${px}:force_original_aspect_ratio=decrease:flags=lanczos`,
      ...(jpg ? ['-q:v', '2'] : ['-compression_level', '100']), '-map_metadata', '-1', '-fflags', '+bitexact', '-flags:v', '+bitexact', dstAbs];
    const r = spawnSync('ffmpeg', args, { encoding: 'utf8' });
    if (r.status !== 0) throw new Error(`ffmpeg: ${String(r.stderr || '').trim().slice(0, 200)}`);
    return `ffmpeg lanczos${jpg ? ' q:v 2' : ''}`;
  }
  if (process.platform === 'darwin' && fs.existsSync('/usr/bin/sips')) {
    const r = spawnSync('sips', ['-Z', String(px), srcAbs, '--out', dstAbs], { encoding: 'utf8' });
    if (r.status !== 0) throw new Error(`sips: ${String(r.stderr || '').trim().slice(0, 200)}`);
    return 'sips -Z';
  }
  throw new Error('kein Bildwerkzeug (ffmpeg, unter macOS sips) gefunden');
}

// Leitet je App die Paket-Icons aus dem gewählten Bild ab (write) bzw. prüft sie gegen das Ableitungsbuch. Liegt das
// gewählte Bild schon im eigenen Paket (knowledge), gilt wie in MAN2 die Byte-Gleichheit mit der Auswahl.
// Ergebnis: { copied: [rel], problems: [{app, kind, detail}], derivations } – problems sind Verstöße (declaration_check).
export function syncPackageIcons(appsRoot, discovered, selection, { write = false } = {}) {
  const copied = [];
  const problems = [];
  const derivations = loadIconDerivations(appsRoot);
  let ledgerChanged = false;
  const fileHash = (abs) => (fs.existsSync(abs) ? sha256(fs.readFileSync(abs)) : null);
  for (const found of discovered) {
    const id = readJson(found.moduleJsonFile).id || path.basename(found.dir);
    const sel = selection[id];
    if (!sel) continue;
    const target = packageIconTarget(sel, found.rel);
    const derived = target.startsWith(`${ICON_PACKAGE_BASENAME}.`);
    if (!derived && fileHash(path.join(found.dir, target)) === sel.sha256) continue;
    const srcAbs = path.join(appsRoot, sel.asset);
    const srcBuf = fs.existsSync(srcAbs) ? fs.readFileSync(srcAbs) : null;
    const srcHash = srcBuf ? sha256(srcBuf) : null;
    if (srcHash !== sel.sha256) {
      problems.push({ app: id, kind: 'icon_package', detail: `${sel.asset} (${sel.source}): Quelle ${srcHash ? `hat sha256 ${srcHash.slice(0, 12)}…, Auswahl nennt ${sel.sha256.slice(0, 12)}…` : 'fehlt'}` });
      continue;
    }
    if (!derived) {
      problems.push({ app: id, kind: 'icon_package', detail: `${found.rel}/${target} weicht von ${sel.asset} ab (eigenes Paketbild)` });
      continue;
    }
    const size = imageSize(srcBuf);
    if (!size) { problems.push({ app: id, kind: 'icon_package', detail: `${sel.asset}: Bildmaße nicht lesbar (nur PNG/JPEG)` }); continue; }
    const srcPx = Math.max(size.width, size.height);
    const outputs = iconOutputs(target, srcPx);
    const entry = derivations.apps[id];
    const fresh = entry && entry.source === sel.asset && entry.source_sha256 === sel.sha256
      && Object.keys(entry.outputs || {}).length === outputs.length
      && outputs.every((o) => entry.outputs[o.rel]?.px === o.px && fileHash(path.join(found.dir, o.rel)) === entry.outputs[o.rel].sha256);
    if (fresh) continue;
    if (!write) {
      problems.push({ app: id, kind: 'icon_package', detail: `${found.rel}/${outputs.map((o) => o.rel).join(' + ')} passt nicht zu ${sel.asset} (sha256 ${sel.sha256.slice(0, 12)}…) laut ${ICON_DERIVATIONS_FILE}: node gen-manifest.mjs (ohne --check) leitet die Icons neu ab` });
      continue;
    }
    try {
      const next = { source: sel.asset, source_sha256: sel.sha256, source_px: srcPx, outputs: {} };
      for (const o of outputs) {
        const dstAbs = path.join(found.dir, o.rel);
        fs.mkdirSync(path.dirname(dstAbs), { recursive: true });
        let tool = 'copy';
        if (srcPx <= o.px) fs.copyFileSync(srcAbs, dstAbs); else tool = resizeIcon(srcAbs, dstAbs, o.px);
        next.outputs[o.rel] = { px: o.px, sha256: sha256(fs.readFileSync(dstAbs)), tool };
        copied.push(`${found.rel}/${o.rel}`);
      }
      derivations.apps[id] = next;
      ledgerChanged = true;
    } catch (err) {
      problems.push({ app: id, kind: 'icon_package', detail: `${sel.asset}: Ableitung gescheitert (${err.message})` });
    }
  }
  if (write && ledgerChanged) {
    const file = path.join(appsRoot, ICON_DERIVATIONS_FILE);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, stableStringify(derivations));
  }
  return { copied, problems, derivations };
}

export function deriveStyles(files) {
  return files['index.css'] ? ['index.css'] : [];
}

function globToRe(pattern) {
  return new RegExp(`^${pattern.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '[^/]*')}$`);
}

export function buildShellManifest(appsRoot, baseRelease) {
  const files = {};
  const todo = [];
  const take = (rel) => {
    const parts = rel.split('/');
    // lab/ = Prüfstand (Testbilder, Demo-Apps, Lab-Server) und Tests gehören nicht ins Artefakt.
    if (parts.includes('lab') || isTestPath(rel) || rel.endsWith('.md')) return;
    // Deckungsgleich mit dem Slot-Bauer (scripts/build-shell-artifact.mjs): was der Bauer ausschließt (z. B. Generatoren
    // wie theme/build-theme.mjs), steht auch nicht im Shell-Manifest – sonst verweist das Manifest auf fehlende Dateien.
    if (isExcludedRuntimePath(rel)) return;
    files[rel] = sha256(fs.readFileSync(path.join(appsRoot, rel)));
  };
  for (const tree of SHELL_TREES) for (const rel of walk(path.join(appsRoot, tree))) take(`${tree}/${rel}`);
  for (const rel of SHELL_SINGLE_FILES) if (fs.existsSync(path.join(appsRoot, rel))) take(rel); else todo.push(`${rel} fehlt.`);
  const sharedNames = fs.existsSync(path.join(appsRoot, 'shared')) ? fs.readdirSync(path.join(appsRoot, 'shared')).sort() : [];
  const rootNames = fs.readdirSync(appsRoot).sort();
  for (const pattern of SHELL_SHARED_PATTERNS) {
    const re = globToRe(pattern);
    const hits = [
      ...sharedNames.filter((n) => re.test(n) && fs.statSync(path.join(appsRoot, 'shared', n)).isFile()).map((n) => `shared/${n}`),
      ...rootNames.filter((n) => re.test(n) && fs.statSync(path.join(appsRoot, n)).isFile()).map((n) => n),
    ];
    if (!hits.length) todo.push(`Dateigrenze nennt ${pattern}, Datei fehlt unter shared/ und business-os/.`);
    for (const rel of hits) take(rel);
  }
  // Abschluss über relative Importe: jede Datei, die eine Shell-Datei statisch oder dynamisch relativ importiert
  // (z. B. die Workjet-Brücke ../../../../shared/*.mjs), gehört ins Artefakt – sonst scheitert der Import im SW.
  for (const rel of shellImportClosure(appsRoot, Object.keys(files))) take(rel);
  const entry = fs.existsSync(path.join(appsRoot, 'v4/frame/index.js')) ? 'v4/frame/index.js' : null;
  todo.push('files: Pfade relativ zu src/apps/business-os/; lab/-Prüfstände, Tests und .md sind ausgenommen.');
  todo.push('app.js/app.css/index.html und die übrigen shared-Dateien stehen laut Dateigrenze im Shell-Artefakt; Shell v4 ersetzt app.js (wird am Ende gelöscht) – Liste beim Rückbau kürzen.');
  todo.push('app_ver/author/created_at setzt der Core bei der Veröffentlichung (§9.1, §9.2).');
  return {
    _format: MANIFEST_FORMAT,
    _generator: GENERATOR_ID,
    _source_dir: 'src/apps/business-os',
    _notes: [
      '§10.2a Nr. 8: die Shell ist selbst app_id \'shell\' mit eigenem Manifest; keine eigenen Collections.',
      'Dateigrenze: ~/.local/state/ctox-sync-v4/docs/dateigrenzen.md Abschnitt „Shell v4“; v4/frame, v4/shell, v4/source, v4/sw.js (§16.15).',
      'entry: v4/frame/index.js exportiert startShell(kernel, {restored}) / stopShell() (§16.15).',
    ],
    app_id: SHELL_APP_ID,
    title: 'Shell v4',
    version: null,
    app_ver: null,
    author: null,
    created_at: null,
    entry,
    files,
    collections: {},
    migrations: {},
    libs: Object.fromEntries(loadStandardLibs(appsRoot).map((lib) => [lib.name, lib.major])),
    shared_exports: SHELL_SHARED_EXPORTS.filter((rel) => {
      if (files[rel]) return true;
      todo.push(`shared_exports nennt ${rel}, Datei fehlt im Shell-Artefakt.`);
      return false;
    }),
    foreign_collections: Object.fromEntries(Object.entries(SHELL_FOREIGN).sort(([a], [b]) => a.localeCompare(b))
      .map(([name, f]) => [name, { owner_app: f.owner_app, ops: sortOps(f.ops), _evidence: { sources: [...f.sources] } }])),
    exports: {},
    handlers: {},
    triggers: [],
    command_rights: {},
    commands_out: SHELL_COMMANDS_OUT.map((c) => ({ app: c.app, type: c.type })).sort((a, b) => a.app.localeCompare(b.app) || a.type.localeCompare(b.type)),
    _commands_out_evidence: Object.fromEntries(SHELL_COMMANDS_OUT.map((c) => [`${c.app} ${c.type}`, [...c.sources]]).sort(([a], [b]) => a.localeCompare(b))),
    capabilities_required: Object.keys(SHELL_CAPABILITIES).sort(),
    _capability_evidence: Object.fromEntries(Object.entries(SHELL_CAPABILITIES).sort(([x], [y]) => x.localeCompare(y)).map(([k, v]) => [k, [...v]])),
    legacy: [],
    _legacy_count: 0,
    min_app_api: MIN_APP_API,
    provenance: { base_release: baseRelease, base_hash: baseHash(files), locally_modified: false },
    layout: {},
    styles: [],
    _styles_basis: 'leer: v4/frame/index.js (cssLinkFor) und v4/source/source-mode.js laden frame.css/source-mode.css selbst',
    _todo: todo,
  };
}

// ---------------------------------------------------------------- Bibliotheks-Manifeste (§16.37)

export function loadStandardLibs(appsRoot) {
  const file = path.join(appsRoot, STANDARD_LIBS_FILE);
  if (!fs.existsSync(file)) return [];
  return Object.entries(readJson(file).libs || {})
    .filter(([, v]) => v && v.status === 'approved')
    .map(([key, v]) => {
      const m = /^([a-z0-9][a-z0-9._-]*)@(\d+)$/.exec(key);
      if (!m) throw new Error(`${STANDARD_LIBS_FILE}: Schlüssel ${key} ist nicht <name>@<major>`);
      return { key, name: m[1], major: m[2], dir: String(v.source || '').split(/\s+/)[0].replace(/\/+$/, ''), version: v.version || null };
    })
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
}

// Paketversion "x.y.z" aus package.json ("version") oder dem Kopf von vs/loader.js ("Version: x.y.z", Monaco).
export function packageVersion(dir) {
  for (const rel of LIB_VERSION_PROBES) {
    const file = path.join(dir, rel);
    if (!fs.existsSync(file)) continue;
    if (rel.endsWith('.json')) {
      const v = readJson(file).version;
      if (typeof v === 'string' && /^\d+\.\d+/.test(v)) return { version: v, from: rel };
    } else {
      const m = /Version:\s*(\d+\.\d+(?:\.\d+)?)/.exec(fs.readFileSync(file, 'utf8').slice(0, 4096));
      if (m) return { version: m[1], from: rel };
    }
  }
  return null;
}

const majorMinor = (v) => String(v).split('.').slice(0, 2).concat('0').slice(0, 2).join('.');

export function buildLibManifest(appsRoot, lib, baseRelease) {
  const todo = [];
  const abs = path.join(appsRoot, lib.dir);
  const files = {};
  if (!lib.dir || !fs.existsSync(abs)) todo.push(`Quellordner ${lib.dir || '(leer)'} fehlt.`);
  // Vendor-Bibliothek: jede Datei gehört zum Artefakt (keine Test-/Markdown-Ausnahme wie bei Apps).
  else for (const rel of walk(abs)) files[rel] = sha256(fs.readFileSync(path.join(abs, rel)));
  let version = null;
  let basis = null;
  if (lib.version) {
    if (majorMinor(lib.version).split('.')[0] === lib.major) { version = majorMinor(lib.version); basis = `${STANDARD_LIBS_FILE} version ${lib.version}`; }
    else todo.push(`${STANDARD_LIBS_FILE}: version ${lib.version} passt nicht zur Bindung @${lib.major}.`);
  }
  if (!version) {
    const pv = fs.existsSync(abs) ? packageVersion(abs) : null;
    if (pv && pv.version.split('.')[0] === lib.major) { version = majorMinor(pv.version); basis = `Paketversion ${pv.version} (${pv.from})`; }
    else if (pv) todo.push(`Paketversion ${pv.version} (${pv.from}) passt nicht zur Bindung @${lib.major}.`);
  }
  if (!version) {
    version = `${lib.major}.0`;
    basis = `Ersatzwert ${lib.major}.0: weder ${STANDARD_LIBS_FILE} noch eine Paketversion nennen die Minor`;
    todo.push(`version: Minor unbekannt, ${version} gesetzt – Minor-Zählung des Bibliotheks-Artefakts festlegen (${STANDARD_LIBS_FILE} "version").`);
  }
  todo.push('files: Pfade relativ zum Quellordner; die URL /~a/lib:<name>@<major>/<pfad> löst der SW über die höchste aktive Minor auf.');
  todo.push('app_ver/author/created_at setzt der Core bei der Veröffentlichung (§9.1, §9.2).');
  return {
    _format: LIB_MANIFEST_FORMAT,
    _generator: GENERATOR_ID,
    _source_dir: `src/apps/business-os/${lib.dir}`,
    _notes: [
      '§16.37: Standardbibliothek als eigenes Plattform-Artefakt; gehört NICHT zum Shell-Artefakt. Das Shell-Manifest bindet nur die Major (libs).',
      'Auslieferung dateiweise bei Bedarf über den SW (/~a/lib:<name>@<major>/<pfad>); Kernel holt fehlende Dateien über Spur 4, nie eager.',
    ],
    app_id: `lib:${lib.name}`,
    kind: 'lib',
    title: lib.name,
    major: lib.major,
    version,
    _version_basis: basis,
    app_ver: null,
    author: null,
    created_at: null,
    files,
    provenance: { base_release: baseRelease, base_hash: baseHash(files), locally_modified: false },
    _todo: todo,
  };
}

export function buildLibManifests(appsRoot, baseRelease) {
  return Object.fromEntries(loadStandardLibs(appsRoot).map((lib) => [lib.name, buildLibManifest(appsRoot, lib, baseRelease)]));
}

export const libManifestName = (name) => `lib-${name}.manifest.json`;

// ---------------------------------------------------------------- Berichte (Markdown)

function renderValue(v) {
  if (v === undefined) return '–';
  const s = JSON.stringify(v);
  return s.length > 80 ? `${s.slice(0, 77)}…` : s;
}
const cell = (s) => String(s).replace(/\|/g, '\\|');

export function renderDriftMarkdown(result) {
  const { drift, stats } = result;
  const out = [];
  out.push('# Drift-Bericht Shell v4 Manifeste (generiert, nicht von Hand ändern)', '');
  out.push(`Erzeugt von \`gen-manifest.mjs\`, Basis ${BASE_RELEASE}. a = schema.js (soweit noch vorhanden), b = collections.schema.json.`, '');
  out.push('## Zahlen', '', '| Kennzahl | Wert |', '|---|---|');
  for (const [k, v] of Object.entries(stats)) out.push(`| ${k} | ${v} |`);
  out.push('', `## 1. schema.js vs. collections.schema.json je App (${drift.schema_js_vs_json.length} Fälle)`, '');
  out.push('| App | Collection | Art | Feld | schema.js | collections.schema.json |', '|---|---|---|---|---|---|');
  for (const d of drift.schema_js_vs_json) {
    if (!d.diffs) { out.push(`| ${d.app} | ${d.collection} | ${d.kind} | | | |`); continue; }
    for (const f of d.diffs) {
      const a = f.only_a ? `nur: ${f.only_a.join(', ') || '–'}` : renderValue(f.a);
      const b = f.only_b ? `nur: ${f.only_b.join(', ') || '–'}` : renderValue(f.b);
      out.push(`| ${d.app} | ${d.collection} | ${f.change || 'differs'} | \`${f.field}\` | ${cell(a)} | ${cell(b)} |`);
    }
  }
  out.push('', `## 2. Migrationsstufen schema.js vs. collections.schema.json (${drift.migration_steps.length})`, '');
  out.push('| App | Collection | schema.js | collections.schema.json |', '|---|---|---|---|');
  for (const d of drift.migration_steps) out.push(`| ${d.app} | ${d.collection} | [${d.schema_js_steps.join(',')}] | [${d.json_steps.join(',')}] |`);
  out.push('', `## 3. schema.js nicht ladbar (${drift.schema_js_load_errors.length})`, '');
  out.push('| App | Fehler |', '|---|---|');
  for (const d of drift.schema_js_load_errors) out.push(`| ${d.app} | ${cell(d.error)} |`);
  out.push('', `## 4. Nur im Schema-Vertrag, in keiner collections.schema.json (${drift.contract_only.length})`, '');
  out.push('| Collection | Eigentümer |', '|---|---|');
  for (const d of drift.contract_only) out.push(`| ${d.collection} | ${d.proposed_owner} |`);
  out.push('', `## 5. Ohne definierende App (${drift.no_owner_app.length})`, '');
  out.push('| Collection | Eigentümer | Grundlage |', '|---|---|---|');
  for (const d of drift.no_owner_app) out.push(`| ${d.collection} | ${d.proposed_owner} | ${cell(d.basis)} |`);
  out.push('', `## 6. Mehrere Definierer in collections.schema.json (${drift.multiple_owners.length})`, '');
  out.push('| Collection | Definierer | Eigentümer |', '|---|---|---|');
  for (const d of drift.multiple_owners) out.push(`| ${d.collection} | ${d.definers.join(', ')} | ${d.owner} |`);
  out.push('', `## 7. Abweichende Kopien derselben Collection über Apps (${drift.copy_divergence.length})`, '');
  out.push('| Collection | Eigentümer | Variante (Quellen) | Feld | Variante | Referenz |', '|---|---|---|---|---|---|');
  for (const d of drift.copy_divergence) {
    for (const v of d.variants) {
      if (!v.diffs_vs_reference.length) { out.push(`| ${d.collection} | ${d.owner} | ${v.sources.join(', ')} | (= Referenz ${d.reference}) | | |`); continue; }
      for (const f of v.diffs_vs_reference) {
        const a = f.only_a ? `nur: ${f.only_a.join(', ') || '–'}` : renderValue(f.a);
        const b = f.only_b ? `nur: ${f.only_b.join(', ') || '–'}` : renderValue(f.b);
        out.push(`| ${d.collection} | ${d.owner} | ${v.sources.join(', ')} | \`${f.field}\` | ${cell(a)} | ${cell(b)} |`);
      }
    }
  }
  out.push('', `## 8. Eigentümer weicht von Namenspräfix ab (${drift.prefix_mismatch.length})`, '');
  out.push('| Collection | Eigentümer | Präfix legt nahe | Grundlage |', '|---|---|---|---|');
  for (const d of drift.prefix_mismatch) out.push(`| ${d.collection} | ${d.owner} | ${d.prefix_suggests} | ${cell(d.basis)} |`);
  out.push('', `## 9. Eigene Collections ohne Schema (${drift.schema_missing.length})`, '');
  out.push('| App | Collection |', '|---|---|');
  for (const d of drift.schema_missing) out.push(`| ${d.app} | ${d.collection} |`);
  out.push('', `## 10. Befehlstypen mit Handlern in mehreren Apps (${drift.handler_conflicts.length})`, '');
  out.push('| Befehlstyp | Apps |', '|---|---|');
  for (const d of drift.handler_conflicts) out.push(`| ${d.command_type} | ${d.apps.join(', ')} |`);
  out.push('', `## 11. Plattformzugriffe auf exports beschränkt (${drift.platform_clamps.length})`, '');
  out.push('| App | Collection | gewünscht | gewährt | Grund |', '|---|---|---|---|---|');
  for (const d of drift.platform_clamps) out.push(`| ${d.app} | ${d.collection} | ${d.wanted.join(',')} | ${d.granted.join(',') || '–'} | ${d.reason} |`);
  out.push('', `## 12. foreign ohne passendes exports (${drift.foreign_without_export.length}, Soll 0)`, '');
  out.push('| App | Collection | Eigentümer | fehlende ops |', '|---|---|---|---|');
  for (const d of drift.foreign_without_export) out.push(`| ${d.app} | ${d.collection} | ${d.owner} | ${d.missing.join(',')} |`);
  out.push('');
  return out.join('\n');
}

export function renderReportM2(result) {
  const { manifests, stats, drift, capabilities, platform } = result;
  const apps = Object.values(manifests).filter((m) => m.app_id !== SHELL_APP_ID).sort((a, b) => a.app_id.localeCompare(b.app_id));
  const out = [];
  out.push('# Manifest v4 – Bericht Strang M2 (generiert von gen-manifest.mjs, nicht von Hand ändern)', '');
  out.push('Quellen: module.json, collections.schema.json (Vorrang v4/schemas/<app_id>.collections.schema.json), handlers/*.js,',
    'Code (commands_out, Trigger-Exporte, net.ingest), je App v4-manifest.json (commands_in, exports, Rechte, multi_instance, Fremdzugriffs-ops, Trigger),',
    `ownership-baseline.json (eingefrorene Eigentümer-Zuordnung), Plattform-exports aus ${platform[PLATFORM_OWNER].source}.`,
    'Prüfung: `node --test src/apps/business-os/v4/tools/manifest/gen-manifest.test.mjs`; `node src/apps/business-os/v4/tools/manifest/gen-manifest.mjs --check`.', '');
  out.push('## Zahlen', '', '| Merkmal | Wert |', '|---|---|');
  out.push(`| Apps (modules + customer-modules) | ${stats.apps} |`);
  out.push(`| Manifeste gesamt (Apps + shell) | ${stats.manifests_total} |`);
  out.push(`| Collections gesamt / App-eigen / ctox-platform | ${stats.collections_total} / ${stats.owned_by_apps} / ${stats.owned_by_platform} |`);
  out.push(`| Handler gesamt (Apps mit Handlern) | ${stats.handlers_total} (${stats.apps_with_handlers}) |`);
  out.push(`| Legacy-Rest gesamt (ctx.cap.legacy) | ${stats.legacy_total} |`);
  out.push(`| Capabilities gesamt (verschieden, ohne legacy) | ${stats.capabilities_distinct} |`);
  out.push(`| Trigger | ${stats.triggers_total} |`);
  out.push(`| command_rights-Einträge | ${stats.command_rights_entries} |`);
  out.push(`| foreign-Deklarationen (davon write / nur Code-Scan) | ${stats.foreign_declarations} (${stats.foreign_write_declarations} / ${stats.foreign_code_only}) |`);
  out.push(`| **foreign ohne passendes exports (Soll 0)** | **${stats.foreign_without_export}** |`);
  out.push(`| Plattformzugriffe auf exports beschränkt | ${stats.platform_clamps} |`);
  out.push(`| Befehlstypen mit Handlern in mehreren Apps | ${stats.handler_conflicts} |`);
  out.push(`| eigene Collections ohne Schema | ${stats.schema_missing} |`);
  out.push(`| schema.js noch vorhanden / nicht ladbar | ${stats.schema_js_present} / ${stats.schema_js_load_errors} |`);
  out.push(`| Shell-Dateien | ${stats.shell_files} |`);
  out.push(`| Apps mit styles / write_guard-Collections | ${stats.apps_with_styles} / ${stats.write_guards} |`);
  out.push(`| offene _todo gesamt | ${stats.todo_total} |`);
  out.push(`| Deklarationsdateien v4-manifest.json | ${stats.declarations} |`);
  out.push(`| commands_out / commands_in | ${stats.commands_out_total} / ${stats.commands_in_total} |`);
  out.push(`| **commands ohne Gegenstück (Soll 0)** | **${stats.commands_unmatched}** |`);
  out.push(`| ctx.command mit nicht auflösbarem Ziel | ${stats.commands_out_unresolved} |`);
  out.push(`| **UI-Befehle ohne Route (E40, Soll 0)** / mehrdeutig | **${stats.commands_unrouted}** / ${stats.commands_out_ambiguous} |`);
  out.push(`| Plattformroute gemeldet (PLATFORM_ROUTES_PENDING) / commands_in aus UI abgeleitet | ${stats.commands_routes_pending} / ${stats.commands_in_derived} |`);
  out.push(`| net.ingest-Anmeldungen | ${stats.net_ingest_total} |`);
  out.push(`| Apps mit multi_instance:false | ${stats.multi_instance_false} |`);
  out.push(`| Icons: Rasterbild im Paket / nur SVG / generiert (MAN2) | ${stats.icon_raster} / ${stats.icon_svg} / ${stats.icon_generated.length}${stats.icon_generated.length ? ` (${stats.icon_generated.join(', ')})` : ''} |`, '');
  out.push('## Je App', '', '| App | Handler | Legacy-Rest | Capabilities | Trigger | command_rights | foreign | exports | _todo |', '|---|---:|---:|---:|---:|---:|---:|---:|---:|');
  for (const m of apps) {
    out.push(`| ${m.app_id} | ${Object.keys(m.handlers).length} | ${m.legacy.length} | ${m.capabilities_required.filter((c) => !c.startsWith('legacy.')).length} | ${m.triggers.length} | ${Object.keys(m.command_rights).length} | ${Object.keys(m.foreign_collections).length} | ${Object.keys(m.exports).length} | ${m._todo.length} |`);
  }
  out.push('', '## Legacy-Rest je App', '');
  for (const m of apps.filter((x) => x.legacy.length)) out.push(`- ${m.app_id} (${m.legacy.length}): ${m.legacy.map((t) => `\`${t}\``).join(', ')}`);
  out.push('', `## Capabilities gesamt (${capabilities.length})`, '');
  const users = {};
  for (const m of apps) for (const c of m.capabilities_required) if (!c.startsWith('legacy.')) (users[c] ||= []).push(m.app_id);
  out.push('| Capability | Apps |', '|---|---|');
  for (const c of capabilities) out.push(`| \`${c}\` | ${users[c].join(', ')} |`);
  out.push('', '## Trigger', '', '| App | Handler | Auslöser |', '|---|---|---|');
  for (const m of apps) for (const t of m.triggers) out.push(`| ${m.app_id} | ${t.handler} | ${t.on ? `on ${t.on.collection} ${t.on.op}` : `every ${t.every}`} |`);
  out.push('', '## App-übergreifende Befehle (§16.40)', '', '| Absender | Ziel-App | Typ | Beleg |', '|---|---|---|---|');
  for (const m of apps) for (const c of m.commands_out) out.push(`| ${m.app_id} | ${c.app} | \`${c.type}\` | ${(m._commands_out_evidence?.[`${c.app} ${c.type}`] || []).join(', ')} |`);
  out.push('', '| Empfänger | Typ | from |', '|---|---|---|');
  for (const m of apps) for (const c of m.commands_in) out.push(`| ${m.app_id} | \`${c.type}\` | ${c.from === '*' ? '*' : c.from.join(', ')} |`);
  out.push('', `Ohne Gegenstück (${drift.commands_unmatched.length}, Soll 0):`);
  for (const d of drift.commands_unmatched) out.push(`- ${d.from} → ${d.app} \`${d.type}\`: ${d.reason}`);
  out.push('', `Nicht auflösbares Ziel (${drift.commands_out_unresolved.length}):`);
  for (const d of drift.commands_out_unresolved) out.push(`- ${d.app} ${d.file}:${d.line} type=\`${d.type_expr}\` module=\`${d.module_expr}\``);
  const shellM = manifests[SHELL_APP_ID];
  if (shellM?.commands_out?.length) {
    out.push('', 'Shell (SHELL_COMMANDS_OUT + UI-Scan v4/frame, v4/shell, v4/source):');
    for (const c of shellM.commands_out) out.push(`- shell → ${c.app} \`${c.type}\`: ${(shellM._commands_out_evidence?.[`${c.app} ${c.type}`] || []).join('; ')}`);
  }
  out.push('', `### commands_in aus der UI-Nutzung abgeleitet (E40, je Absender, nie '*') (${drift.commands_in_derived.length})`, '');
  for (const d of drift.commands_in_derived) out.push(`- ${d.app} ← ${d.from} \`${d.type}\`: ${d.evidence.join(', ')}`);
  if (!drift.commands_in_derived.length) out.push('keine');
  out.push('', `### UI-Befehle ohne Route: weder Plattformroute (${COMMAND_INVENTORY_FILE} + Registry) noch Handler (E40, ${drift.commands_unrouted.length}, Soll 0, rot)`, '');
  for (const d of drift.commands_unrouted) out.push(`- ${d.app} \`${d.type}\`: ${d.evidence.join(', ')}`);
  if (!drift.commands_unrouted.length) out.push('keine');
  out.push('', `### Mehrdeutig: Handler in mehreren Apps (${drift.commands_out_ambiguous.length}, Soll 0, rot)`, '');
  for (const d of drift.commands_out_ambiguous) out.push(`- ${d.app} \`${d.type}\` → ${d.handlers.join(' | ')}: ${d.evidence.join(', ')}`);
  if (!drift.commands_out_ambiguous.length) out.push('keine');
  out.push('', `### An den Kern gemeldet, noch keine Plattformroute (PLATFORM_ROUTES_PENDING, ${drift.commands_routes_pending.length})`, '');
  for (const d of drift.commands_routes_pending) out.push(`- \`${d.type}\` (${d.basis}): ${d.app ? `${d.app} ${d.evidence.join(', ')}` : 'im UI-Scan keine Fundstelle als Befehlstyp'}`);
  out.push('', `## foreign ohne passendes exports (${drift.foreign_without_export.length}, Soll 0)`, '');
  for (const d of drift.foreign_without_export) out.push(`- ${d.app}: ${d.collection} (${d.owner}) fehlt ${d.missing.join(',')}`);
  if (!drift.foreign_without_export.length) out.push('keine');
  out.push('', `## Plattformzugriffe auf exports beschränkt (${drift.platform_clamps.length})`, '');
  for (const d of drift.platform_clamps) out.push(`- ${d.app}: ${d.collection} gewünscht ${d.wanted.join(',')}, gewährt ${d.granted.join(',') || '–'} (${d.reason})`);
  out.push('', `## Befehlstypen mit Handlern in mehreren Apps (${drift.handler_conflicts.length})`, '');
  for (const d of drift.handler_conflicts) out.push(`- \`${d.command_type}\`: ${d.apps.join(', ')}`);
  out.push('', '## Offene _todo (app-spezifisch; die drei Standardpunkte rights/client_limit/app_ver stehen in jedem Manifest)', '');
  const standard = /^(rights je Collection|client_limit\/byte_budget|app_ver\/author\/created_at)/;
  for (const m of Object.values(manifests).sort((a, b) => a.app_id.localeCompare(b.app_id))) {
    const own = m._todo.filter((t) => !standard.test(t));
    const colTodos = Object.entries(m.collections).filter(([, c]) => c._todo).map(([n, c]) => `${n}: ${c._todo.join(' ')}`);
    if (!own.length && !colTodos.length) continue;
    out.push(`### ${m.app_id}`, '');
    for (const t of own) out.push(`- ${t}`);
    for (const t of colTodos) out.push(`- Collection ${t}`);
    out.push('');
  }
  out.push('## Regeln dieses Strangs', '',
    '1. customer-modules/* werden erfasst; gelöscht werden nur Manifeste mit `_generator` = gen-manifest.mjs, die nicht mehr erzeugt werden.',
    '2. schema.js ist optional (Lade-Fehler landen im Drift-Bericht); Schemaquelle allein collections.schema.json bzw. v4/schemas/<app_id>.collections.schema.json; A9-`migrations` und Alt-`migration_strategies` werden gelesen.',
    '3. Plattform-Collections gehören ctox-platform; Apps deklarieren sie nur gemäß Plattform-exports (business_module_catalog → business_apps/business_app_versions read); überschießende ops werden gekappt und gelistet.',
    '4. handlers = vorhandene handlers/*.js ohne _-Präfix; capabilities_required = im Handler-Code aufgerufene ctx.cap-Pfade (ohne Präfix „ctx.cap.“, wie in den V4-HANDLERS-Vorschlägen) plus `legacy.<typ>` je Legacy-Typ; Handler-Dateien mit „test“ im Befehlstyp sind keine Testdateien.',
    '5. Shell-Manifest: v4/{frame,shell,source}/** + v4/sw.js + Shell-Dateien laut Dateigrenze; lab/-Prüfstände, Tests und .md ausgenommen.',
    '6. styles (§16.19 Nr. 6) = index.css der App (+ styles der Deklaration); write_guard (§16.21) nur, wo ein Rust-Schreibwächter belegt ist (OLG-Leads, outbound_field_review.rs:88).',
    '7. commands_out (§16.40) aus ctx.command im Handler-Code (Literal/Konstante, module-Option, Weiterleitung command.type) und aus UI-Code (shared/context-chat.js; payload.module mit Handler-Typ der Ziel-App); commands_in aus v4-manifest.json plus je konkretem Absender aus der UI-Nutzung (E39/E40, nie \'*\'); jede Seite ohne Gegenstück ist ein Befund.',
    '7a. UI-Befehle (E39/E40): Typ-Position von api.command bzw. app-lokaler Hüllfunktionen (transitiv); eigener Handler → eigene Route, Plattformroute (Inventur exact_control_types/control_prefixes + Registry ctox.app.*, ctox.coding.*) → keine Deklaration, genau eine fremde Handler-App → commands_out/commands_in, sonst rot (commands_unrouted/commands_out_ambiguous) außer PLATFORM_ROUTES_PENDING.',
    '8. Trigger aus `export const triggers` im Handler oder v4-manifest.json; on-Trigger nur auf eigene oder foreign-watch-Collections. net.ingest-Zustelltypen bekommen command_rights {roles: []} (nur Core-Zustellung).',
    '');
  return out.join('\n');
}

// ---------------------------------------------------------------- Ausgabe

export function renderOutputs(result) {
  const files = {};
  for (const [id, m] of Object.entries(result.manifests)) files[`manifests/${id}.manifest.json`] = stableStringify(m);
  for (const [name, m] of Object.entries(result.libs || {})) files[`manifests/${libManifestName(name)}`] = stableStringify(m);
  files['report/ownership.json'] = stableStringify({ platform: result.platform, ownership: result.ownership });
  files['report/drift.json'] = stableStringify(result.drift);
  files['report/stats.json'] = stableStringify(result.stats);
  files['report/DRIFT.md'] = renderDriftMarkdown(result);
  files['tool/REPORT-M2.md'] = renderReportM2(result);
  return files;
}

// Schreibt bzw. prüft die Ausgaben. Gelöscht werden nur Manifeste, die dieser Generator erzeugt hat
// (`_generator` = GENERATOR_ID) und die nicht mehr erzeugt werden; fremde Manifeste bleiben unangetastet.
export function writeOutputs(result, { outDir = DEFAULT_OUT, reportDir = DEFAULT_REPORT_OUT, toolDir = DEFAULT_TOOL_OUT, check = false } = {}) {
  const outputs = renderOutputs(result);
  const target = (key) => {
    if (key.startsWith('manifests/')) return path.join(outDir, key.slice('manifests/'.length));
    if (key.startsWith('tool/')) return path.join(toolDir, key.slice('tool/'.length));
    return path.join(reportDir, key.slice('report/'.length));
  };
  const mismatches = [];
  for (const [key, text] of Object.entries(outputs)) {
    const file = target(key);
    if (check) {
      const current = fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : null;
      if (current !== text) mismatches.push(file);
    } else {
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(file, text);
    }
  }
  const expected = new Set([
    ...Object.keys(result.manifests).map((id) => `${id}.manifest.json`),
    ...Object.keys(result.libs || {}).map(libManifestName),
  ]);
  const stale = [];
  if (fs.existsSync(outDir)) {
    for (const name of fs.readdirSync(outDir).sort()) {
      if (!name.endsWith('.manifest.json') || expected.has(name)) continue;
      let gen = null;
      try { gen = readJson(path.join(outDir, name))._generator; } catch { gen = null; }
      if (gen !== GENERATOR_ID) continue;
      stale.push(name);
      if (!check) fs.rmSync(path.join(outDir, name));
    }
  }
  if (check) mismatches.push(...stale.map((n) => path.join(outDir, n)));
  return { mismatches, removed: check ? [] : stale };
}

function parseArgs(argv) {
  const args = { check: false };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--check') args.check = true;
    else if (a === '--repo') args.repo = argv[++i];
    else if (a === '--out') args.out = argv[++i];
    else if (a === '--report-out') args.reportOut = argv[++i];
    else if (a === '--analysis') args.analysis = argv[++i];
    else if (a === '--base-release') args.baseRelease = argv[++i];
    else if (a === '--platform-manifest') args.platformManifest = argv[++i];
    else if (a === '--shell-libs') args.shellLibs = true;
    else throw new Error(`unbekannte Option ${a}`);
  }
  return args;
}

// Nur Shell- und Bibliotheks-Manifeste (ohne App-Manifeste und Berichte), z. B. während andere Stränge an App-Dateien
// arbeiten: node gen-manifest.mjs --shell-libs [--check] [--out DIR] [--repo DIR]. Löscht nichts.
export function generateShellAndLibs(options = {}) {
  const appsRoot = path.join(path.resolve(options.repo || DEFAULT_REPO), 'src/apps/business-os');
  const baseRelease = options.baseRelease || BASE_RELEASE;
  return { shell: buildShellManifest(appsRoot, baseRelease), libs: buildLibManifests(appsRoot, baseRelease) };
}

export function writeShellAndLibs(result, { outDir = DEFAULT_OUT, check = false } = {}) {
  const outputs = { [`${SHELL_APP_ID}.manifest.json`]: stableStringify(result.shell) };
  for (const [name, m] of Object.entries(result.libs)) outputs[libManifestName(name)] = stableStringify(m);
  const mismatches = [];
  for (const [name, text] of Object.entries(outputs)) {
    const file = path.join(outDir, name);
    if (check) { if (!fs.existsSync(file) || fs.readFileSync(file, 'utf8') !== text) mismatches.push(file); }
    else { fs.mkdirSync(outDir, { recursive: true }); fs.writeFileSync(file, text); }
  }
  return { mismatches, written: check ? [] : Object.keys(outputs) };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.shellLibs) {
    const r = generateShellAndLibs(args);
    const { mismatches, written } = writeShellAndLibs(r, { outDir: path.resolve(args.out || DEFAULT_OUT), check: args.check });
    for (const file of mismatches) console.error(`abweichend: ${file}`);
    console.log(JSON.stringify({ shell_files: Object.keys(r.shell.files).length, libs: Object.fromEntries(Object.entries(r.libs).map(([n, m]) => [n, { version: m.version, files: Object.keys(m.files).length }])), written }));
    if (args.check && mismatches.length) process.exit(1);
    return;
  }
  const result = await generate({ ...args, syncIcons: !args.check });
  for (const rel of result.icons_copied) console.error(`icon kopiert: ${rel}`);
  const { mismatches } = writeOutputs(result, {
    outDir: path.resolve(args.out || DEFAULT_OUT),
    reportDir: path.resolve(args.reportOut || DEFAULT_REPORT_OUT),
    check: args.check,
  });
  for (const file of mismatches) console.error(`abweichend: ${file}`);
  for (const v of result.skill_check.violations) console.error(`skill: ${v.app} ${v.kind} ${v.ref ?? v.path}${v.file ? ` ${v.file}:${v.line}` : ''}`);
  for (const v of result.declaration_check.violations) console.error(`deklaration: ${v.app} ${v.kind} ${v.detail}`);
  for (const v of result.route_check.violations) console.error(`route: ${v.app} ${v.kind} ${v.detail}`);
  console.log(JSON.stringify(result.stats));
  if ((args.check && mismatches.length) || result.skill_check.violations.length || result.declaration_check.violations.length || result.route_check.violations.length) process.exit(1);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((err) => { console.error(err); process.exit(2); });
}
