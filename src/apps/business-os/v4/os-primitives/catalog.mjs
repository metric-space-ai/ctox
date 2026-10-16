// Origin: CTOX
// License: Apache-2.0
//
// Shell v4 – CAPA "Shell v4": the single source of truth for the CTOX-OS primitive
// catalogue and the classification of every manifest capability. Consumed by
// MAPPING.md's generator (gen-mapping.mjs) and by the CI guard check-caps.mjs.
//
// Owner directive (Michael, 15.10.): CTOX provides only operating-system-like
// interfaces; application logic must NEVER live in the core. A core capability is
// legitimate only when it is generic: no app id, no app collection, no domain term
// in its name or semantics. Everything else is app logic (UI + JS handlers over the
// app's own collections) or platform (ctox-platform: install, rights, queue
// projection, business_commands).
//
// Reading: ctox-sync-v4-architektur.md §9.4, §16.8, §16.13, §16.16, §16.21;
// shell-v4/analysis/E-befehlstypen.md §3.

// ---------------------------------------------------------------------------
// 1. The OS-primitive catalogue (<= ~20 generic interfaces). Keys are the
//    primitive ids; `ns` lists the capability namespaces / exact verb names the
//    core exposes for that primitive. A capabilities_required entry is allowed in
//    the core ONLY if it matches one of these (prefix match on a trailing ".", or
//    exact match). See CATALOG.md for signatures, policy and rationale.
// ---------------------------------------------------------------------------
export const PRIMITIVES = {
  files:          ['files.', 'file.', 'blob.', 'artifact.write'],
  'net.http':     ['net.http', 'web.fetch', 'http.fetch'],
  browser:        ['browser.', 'web.capture', 'research.person'],
  'process.run':  ['process.run', 'process.', 'knowledge.command',
                   'scrape.execute_test', 'scrape.run_file', 'scrape.reprobe_keep_script',
                   'external_sql.sync', 'external_sql.write'],
  'mail.transport': ['mail.send', 'mail.server.', 'mail.account.upsert',
                     'mail.communication.upsert', 'mail.message.annotate',
                     'mail.default_sender', 'mail.delivery_log.', 'mail.validate_contacts',
                     'mail.channel.', 'mail.thread.read'],
  'agent.task':   ['agent.task.', 'agent.workspace.', 'scrape.repair_task.create'],
  'agent.turn':   ['agent.turn.', 'agent.coding.'],
  'agent.queue':  ['agent.queue.'],
  schedule:       ['schedule.'],
  secrets:        ['secret.', 'secrets.'],
  directory:      ['directory.', 'identity.'],
  audit:          ['audit.'],
  config:         ['config.', 'runtime_state.'],
  doc:            ['doc.', 'office.'],
  speech:         ['speech.'],
  // knowledge.query (§16.30, vormals knowledge.command): nativer Knowledge-Datenspeicher (`ctox knowledge data …`).
  dataframe:      ['dataframe.', 'knowledge.query', 'knowledge.project', 'knowledge.rows', 'knowledge.snapshot.',
                   'knowledge.table.', 'knowledge.tables.', 'iot.datapoints.query'],
  'net.ingest':   ['net.ingest', 'net.ingest.'],
  policy:         ['policy.'],
  lock:           ['lock.', 'fence.'],
  // MAN3: Grant `model` (genau dieser Name, kein Präfix) für ctx.model(req), die Modell-Bindung der App-Laufzeit (#830
  // capability_available("model")). Kein ctx.cap-Pfad: model.run bleibt App-Logik und ist aus den Manifesten entfernt.
  model:          ['model'],
};

// Order of the primitives as presented in CATALOG.md / summaries.
export const PRIMITIVE_ORDER = Object.keys(PRIMITIVES);

// ---------------------------------------------------------------------------
// 2. App-logic and platform classification. Any capability NOT matched by a
//    PRIMITIVES entry is app logic or platform and must leave the core. These
//    tables carry the target home, the effort estimate and the main handler files.
//    Matching is by exact id first, then by the longest matching prefix.
// ---------------------------------------------------------------------------

// Platform (ctox-platform): generic plumbing that spiegelt core state. Allowed in
// the core, but under the platform manifest, never under an app id.
export const PLATFORM = {
  'agent.flow.record': { why: 'Harness-Prozess-Evidenz (harness_flow)', files: ['ctox/handlers/_lib/*'] },
  'command.get':       { why: 'Lesen von business_commands (Plattform-Collection)', files: ['outbound-lead-generation/handlers/_lib/research-writeback.js'] },
};

// App logic: must become app handlers over the app's own collections, driven by
// the OS primitives above. `app` = owning app, `logic` = what the domain logic is,
// `via` = how the target realises it, `effort` = S/M/L, `files` = main handlers.
export const APP_LOGIC = {
  // --- CAP-3b #844: aus dem OS-Katalog entfernt (keine Primitive) -----------
  // model.run und agent.command.settle sind KEINE Capabilities; sie gehören aus
  // capabilities_required entfernt (MAN2): model kommt über die App-Laufzeit
  // ctx.model (#830), der Befehlsabschluss über den Befehls-Lebenszyklus (Rückruf).
  // Bis MAN2 die Manifeste bereinigt hat, stehen sie in der Ratchet-Baseline.
  'model.run': { app: '(App-Laufzeit)', logic: 'Modell-Gateway – kein cap', via: 'ctx.model(req) der App-Laufzeit (#830); aus capabilities_required entfernen', effort: 'S', files: ['iot/handlers/_codegen.js'] },
  'agent.command.settle': { app: '(Befehls-Lebenszyklus)', logic: 'Befehlsabschluss – kein cap', via: 'Rückruf-Befehl/Ergebnis (§16.30); aus capabilities_required entfernen', effort: 'S', files: ['research/handlers/_lib/records.js'] },
  // appsec-pentest: App-Logik auf process.run (Engine über Allowlist-Namen) + files.get (State-Dir bis Cutover unavailable).
  'pentest.run':           { app: 'appsec-pentest', logic: 'Pentest-Engine-Lauf', via: 'process.run (Allowlist-Name) + files; Port blockiert bis files.*-Cutover', effort: 'L', files: ['appsec-pentest/handlers/_lib/args.js'] },
  'pentest.app_audit':     { app: 'appsec-pentest', logic: 'Business-OS-App-Audit', via: 'process.run (Allowlist-Name)', effort: 'L', files: ['appsec-pentest/handlers/ctox.appsec.app.audit.js'] },
  'pentest.exploit_list':  { app: 'appsec-pentest', logic: 'Exploit-Index lesen', via: 'files.get (State-Dir-Index)', effort: 'M', files: ['appsec-pentest/handlers/ctox.appsec.exploit.list.js'] },
  'pentest.exploit_get':   { app: 'appsec-pentest', logic: 'Exploit-Skript lesen', via: 'files.get (State-Dir, Pfadprüfung im Handler)', effort: 'M', files: ['appsec-pentest/handlers/ctox.appsec.exploit.get.js'] },
  'pentest.exploit_verify':{ app: 'appsec-pentest', logic: 'Exploit erneut ausführen', via: 'process.run + files.get', effort: 'L', files: ['appsec-pentest/handlers/ctox.appsec.exploit.verify.js'] },

  // --- iot: assets / dashboards / widgets / rules / alarms / agent (14 ops) ----
  'iot.asset.upsert':   { app: 'iot', logic: 'Asset-Modell (Typ-Deskriptor, Attribut-Deckel)', via: 'iot-Collections + Validierung im iot-Handler; Telemetrie über net.ingest, Zeitreihen über dataframe', effort: 'L', files: ['iot/handlers/ctox.iot.asset.upsert.js'] },
  'iot.asset.delete':   { app: 'iot', logic: 'Asset-Lebenszyklus', via: 'iot-Collections', effort: 'S', files: ['iot/handlers/ctox.iot.asset.delete.js'] },
  'iot.alarm.update':   { app: 'iot', logic: 'Alarmdefinition', via: 'iot-Collections', effort: 'M', files: ['iot/handlers/ctox.iot.alarm.update.js'] },
  'iot.ruleset.save':   { app: 'iot', logic: 'Regelwerk-Parsing/Speicherung', via: 'iot-Collections; Auswertung als every/on-Trigger (§16.20)', effort: 'L', files: ['iot/handlers/ctox.iot.ruleset.save.js'] },
  'iot.ruleset.toggle': { app: 'iot', logic: 'Regelwerk aktiv/inaktiv', via: 'iot-Collections', effort: 'S', files: ['iot/handlers/ctox.iot.ruleset.toggle.js'] },
  'iot.agent.configure':{ app: 'iot', logic: 'Agent-Konfiguration je Asset', via: 'iot-Collections + agent.task', effort: 'M', files: ['iot/handlers/ctox.iot.agent.configure.js'] },
  'iot.dashboard.upsert':{ app: 'iot', logic: 'Dashboard-Komposition', via: 'iot-Collections', effort: 'M', files: ['iot/handlers/ctox.iot.dashboard.upsert.js'] },
  'iot.dashboard.delete':{ app: 'iot', logic: 'Dashboard-Lebenszyklus', via: 'iot-Collections', effort: 'S', files: ['iot/handlers/ctox.iot.dashboard.delete.js'] },
  'iot.widget.upsert':  { app: 'iot', logic: 'Widget-Definition', via: 'iot-Collections', effort: 'M', files: ['iot/handlers/ctox.iot.widget.upsert.js'] },
  'iot.widget.delete':  { app: 'iot', logic: 'Widget-Lebenszyklus', via: 'iot-Collections', effort: 'S', files: ['iot/handlers/ctox.iot.widget.delete.js'] },
  'iot.widget.arrange': { app: 'iot', logic: 'Widget-Anordnung', via: 'iot-Collections', effort: 'S', files: ['iot/handlers/ctox.iot.widget.arrange.js'] },
  'iot.widget.compile_trigger': { app: 'iot', logic: 'Trigger-Kompilierung des Widgets', via: 'iot-Collections + model', effort: 'M', files: ['iot/handlers/ctox.iot.widget.compile_trigger.js'] },
  'iot.widget.generate_render': { app: 'iot', logic: 'Render-Codegen des Widgets', via: 'model + iot-Collections', effort: 'M', files: ['iot/handlers/ctox.iot.widget.generate_render.js'] },
  'iot.widget.pause':   { app: 'iot', logic: 'Widget-Laufzeit anhalten', via: 'iot-Collections', effort: 'S', files: ['iot/handlers/ctox.iot.widget.pause.js'] },

  // --- tickets / support: case state machine -------------------------------
  'ctox.ticket.run':    { app: 'tickets/support', logic: 'Fall-Zustandsautomat, Freigaben, Rückfragen', via: 'ticket-Collections im Handler-Zustandsautomaten; Ausführung über agent.task; Audit über audit', effort: 'L', files: ['tickets/handlers/_tickets.js', 'support/handlers/support.ticket.create_from_conversation.js'] },

  // --- crew (ctox app) ------------------------------------------------------
  'agent.crew.assign':        { app: 'ctox', logic: 'Mitglied an Task binden', via: 'crew-Collections + agent.queue (atomic claim)', effort: 'M', files: ['ctox/handlers/ctox.crew.assign.js'] },
  'agent.crew.members.list':  { app: 'ctox', logic: 'Crew-Register lesen', via: 'crew-Collections (api.query)', effort: 'S', files: ['ctox/handlers/_lib/crew.js'] },
  'agent.crew.member.upsert': { app: 'ctox', logic: 'Crew-Mitglied pflegen', via: 'crew-Collections', effort: 'M', files: ['ctox/handlers/ctox.crew.member.create.js', 'ctox/handlers/ctox.crew.member.update.js'] },
  'agent.crew.memory.update': { app: 'ctox', logic: 'LCM-Continuity/Wissen je Wesen', via: 'crew-Collections bzw. generische versionierte doc-Ablage', effort: 'L', files: ['ctox/handlers/ctox.crew.memory.update.js'] },
  'agent.crew.learning.get':  { app: 'ctox', logic: 'Lernen lesen', via: 'crew-Collections', effort: 'S', files: ['ctox/handlers/_lib/learning.js'] },
  'agent.crew.learning.write':{ app: 'ctox', logic: 'Lernen bestätigen/ändern/löschen', via: 'crew-Collections', effort: 'M', files: ['ctox/handlers/_lib/learning.js'] },

  // --- scrape registry (OLG / outbound) ------------------------------------
  'scrape.list_targets':             { app: 'outbound-lead-generation', logic: 'Ziel-Register lesen', via: 'OLG-Registry-Collections', effort: 'M', files: ['outbound-lead-generation/handlers/outbound.research_source.registry_read.js'] },
  'scrape.account_states':           { app: 'outbound-lead-generation', logic: 'Konto-Zustände je Ziel', via: 'OLG-Registry-Collections', effort: 'S', files: ['outbound-lead-generation/handlers/outbound.research_source.registry_read.js'] },
  'scrape.last_runs':                { app: 'outbound-lead-generation', logic: 'Laufhistorie je Ziel', via: 'OLG-Registry-Collections', effort: 'S', files: ['outbound-lead-generation/handlers/outbound.research_source.registry_read.js'] },
  'scrape.latest_script':            { app: 'outbound-lead-generation', logic: 'Aktuelles Skript je Ziel', via: 'OLG-Registry-Collections + files', effort: 'S', files: ['outbound-lead-generation/handlers/outbound.research_source.registry_read.js'] },
  'scrape.registry.read':            { app: 'outbound', logic: 'Registry-Lesesicht', via: 'OLG-Registry-Collections (foreign read)', effort: 'S', files: ['outbound/handlers/outbound.research_source.registry_read.js'] },
  'scrape.registry_runs':            { app: 'outbound-lead-generation', logic: 'Lauf-Register', via: 'OLG-Registry-Collections', effort: 'S', files: ['outbound-lead-generation/handlers/outbound.update_digest.send_now.js'] },
  'scrape.registered_target_summary':{ app: 'outbound-lead-generation', logic: 'Ziel-Zusammenfassung', via: 'OLG-Registry-Collections', effort: 'S', files: ['outbound-lead-generation/handlers/_lib/research-source-adapter.js'] },
  'scrape.target.registration':      { app: 'outbound', logic: 'Ziel-Registrierung lesen', via: 'OLG-Registry-Collections', effort: 'S', files: ['outbound/handlers/_lib/scrape-generation.js'] },
  'scrape.target.upsert':            { app: 'outbound', logic: 'Ziel-Manifest schreiben', via: 'OLG-Registry-Collections + process.run (scrape upsert-target)', effort: 'M', files: ['outbound/handlers/_lib/scrape-generation.js'] },
  'scrape.target_script_registration':{ app: 'outbound-lead-generation', logic: 'Skript-Registrierung je Ziel', via: 'OLG-Registry-Collections + files', effort: 'M', files: ['outbound-lead-generation/handlers/_lib/research-source-adapter.js'] },
  'scrape.upsert_target':            { app: 'outbound-lead-generation', logic: 'Ziel anlegen/ändern', via: 'OLG-Registry-Collections', effort: 'M', files: ['outbound-lead-generation/handlers/_lib/research-source-adapter.js'] },

  // --- workjet (Phase-11 app / threads) ------------------------------------
  'workjet.command_access_check': { app: 'threads/workjet', logic: 'Privatchat-Zugriffsschutz', via: 'kann zu generischem policy verallgemeinert werden; sonst threads-Handler + policy', effort: 'M', files: ['threads/handlers/_lib.js'] },
  'workjet.supervisor_binding':   { app: 'threads/workjet', logic: 'Supervisor-Bindung lesen', via: 'workjet-Collections (foreign read)', effort: 'M', files: ['threads/handlers/_lib.js'] },

  // --- OLG lead field witnesses --------------------------------------------
  'outbound.field_status_witnesses.read':  { app: 'outbound-lead-generation', logic: 'Feld-Status-Zeugen lesen', via: 'OLG-Collections', effort: 'M', files: ['outbound-lead-generation/handlers/_lib/research-field-status.js'] },
  'outbound.field_status_witnesses.issue': { app: 'outbound-lead-generation', logic: 'Feld-Status-Zeugen ausstellen', via: 'OLG-Collections', effort: 'M', files: ['outbound-lead-generation/handlers/_lib/research-writeback.js'] },

  // --- legacy transition shims (native path until ported) ------------------
  'legacy.outbound.lead.research_writeback': { app: 'outbound-lead-generation', logic: 'Übergangs-Shim auf nativen Pfad', via: 'wird Handler-Logik über agent.task/dataframe', effort: 'L', files: ['outbound-lead-generation/handlers/*'] },
  'legacy.web_stack.person_research':        { app: 'outbound-lead-generation', logic: 'Übergangs-Shim', via: 'wird Handler-Logik über browser', effort: 'L', files: ['outbound-lead-generation/handlers/web_stack.person_research.js'] },
  'legacy.outbound.research_source.reprobe': { app: 'outbound', logic: 'Übergangs-Shim', via: 'wird Handler-Logik über process.run/browser', effort: 'M', files: ['outbound/handlers/outbound.research_source.reprobe.js'] },
  'legacy.outbound.research_source.test':    { app: 'outbound', logic: 'Übergangs-Shim', via: 'wird Handler-Logik über process.run/browser', effort: 'M', files: ['outbound/handlers/outbound.research_source.test.js'] },
  'legacy.outbound.sellify.lookup':          { app: 'outbound', logic: 'Übergangs-Shim (liest replizierte Sellify-Daten)', via: 'wird Handler-Logik über net.http + App-Collections', effort: 'M', files: ['outbound/handlers/outbound.sellify.lookup.js'] },
  'legacy.outbound.update_digest.send_now':  { app: 'outbound', logic: 'Übergangs-Shim', via: 'wird Handler-Logik über mail.transport', effort: 'M', files: ['outbound/handlers/outbound.update_digest.send_now.js'] },
};

// Prefix fallbacks for app-logic families, so a newly generated cap still
// classifies (used only when no exact APP_LOGIC entry exists).
export const APP_LOGIC_PREFIX = {
  'iot.asset.':     { app: 'iot', effort: 'M' },
  'iot.dashboard.': { app: 'iot', effort: 'M' },
  'iot.widget.':    { app: 'iot', effort: 'M' },
  'iot.ruleset.':   { app: 'iot', effort: 'M' },
  'iot.alarm.':     { app: 'iot', effort: 'M' },
  'iot.agent.':     { app: 'iot', effort: 'M' },
  'ctox.ticket.':   { app: 'tickets/support', effort: 'L' },
  'agent.crew.':    { app: 'ctox', effort: 'M' },
  'scrape.':        { app: 'outbound-lead-generation', effort: 'M' },
  'workjet.':       { app: 'threads/workjet', effort: 'M' },
  'outbound.':      { app: 'outbound-lead-generation', effort: 'M' },
  'pentest.':       { app: 'appsec-pentest', effort: 'L' },
  'legacy.':        { app: '(ursprüngliche App)', effort: 'L' },
};

// ---------------------------------------------------------------------------
// 3. Classifier. Returns { kind: 'os'|'platform'|'app', primitive?, ...info }.
// ---------------------------------------------------------------------------
function longestPrefix(cap, table) {
  let best = null;
  for (const key of Object.keys(table)) {
    if (key.endsWith('.') ? cap.startsWith(key) : cap === key) {
      if (!best || key.length > best.length) best = key;
    }
  }
  return best;
}

export function classify(cap) {
  // OS primitive?
  for (const [prim, nss] of Object.entries(PRIMITIVES)) {
    for (const ns of nss) {
      if (ns.endsWith('.') ? cap.startsWith(ns) : cap === ns) {
        return { kind: 'os', primitive: prim };
      }
    }
  }
  // Platform?
  if (PLATFORM[cap]) return { kind: 'platform', ...PLATFORM[cap] };
  // App logic (exact then prefix)?
  if (APP_LOGIC[cap]) return { kind: 'app', ...APP_LOGIC[cap] };
  const pfx = longestPrefix(cap, APP_LOGIC_PREFIX);
  if (pfx) return { kind: 'app', ...APP_LOGIC_PREFIX[pfx], logic: '(abgeleitet)', via: '(abgeleitet)', files: [] };
  // Unknown -> treated as a violation by the guard, surfaced for triage.
  return { kind: 'unknown' };
}
