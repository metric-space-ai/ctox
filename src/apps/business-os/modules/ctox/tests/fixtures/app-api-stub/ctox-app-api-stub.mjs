// Generated from src/core/business_os/business_command_inventory.json. Check: tests/e39-inventory.test.mjs.
const COMMAND_INVENTORY = {
  "exact_control_types": [
    "ctox.app.access.grant",
    "ctox.app.access.revoke",
    "ctox.app.action.run",
    "ctox.app_store.install",
    "ctox.app_store.uninstall",
    "ctox.business_os.audit.list",
    "ctox.business_os.audit.retention",
    "ctox.business_os.audit.retention_policy.set",
    "ctox.business_os.backup.restore_drill",
    "ctox.business_os.branding.update",
    "ctox.business_os.support.export_diagnostics",
    "ctox.business_os.user.upsert",
    "ctox.business_os.why",
    "ctox.coding.models",
    "ctox.coding.turn",
    "ctox.command.cancel",
    "ctox.crew.assign",
    "ctox.crew.learning.confirm",
    "ctox.crew.learning.delete",
    "ctox.crew.learning.update",
    "ctox.crew.member.create",
    "ctox.crew.member.update",
    "ctox.crew.memory.update",
    "ctox.device.unbind",
    "ctox.file.export",
    "ctox.file.materialize",
    "ctox.guest.input",
    "ctox.guest.observe",
    "ctox.machine.enroll.begin",
    "ctox.machine.enroll.cancel",
    "ctox.machine.enroll.step",
    "ctox.machine.heartbeat",
    "ctox.mailserver.delete_domain",
    "ctox.mailserver.delete_user",
    "ctox.mailserver.get_config",
    "ctox.mailserver.save_domain",
    "ctox.mailserver.save_runtime",
    "ctox.mailserver.save_user",
    "ctox.maintenance.client_ready",
    "ctox.module.assign_founder",
    "ctox.module.check_updates",
    "ctox.module.delete",
    "ctox.module.install_template",
    "ctox.module.list_versions",
    "ctox.module.release",
    "ctox.module.repair_lifecycle_projection",
    "ctox.module.rollback",
    "ctox.module.rollback_version",
    "ctox.module.save",
    "ctox.module.set_visible",
    "ctox.module.update",
    "ctox.office.settings.save",
    "ctox.plan.step.retry",
    "ctox.provider_subscription.disconnect",
    "ctox.provider_subscription.rotate",
    "ctox.provider_subscription.status",
    "ctox.queue.abort_turn",
    "ctox.queue.block",
    "ctox.queue.capacity",
    "ctox.queue.pause",
    "ctox.queue.release",
    "ctox.queue.retry",
    "ctox.runtime_settings.save",
    "ctox.secret.delete",
    "ctox.secret.list",
    "ctox.secret.put",
    "ctox.source.commit",
    "ctox.source.diff",
    "ctox.source.list_snapshots",
    "ctox.source.load",
    "ctox.source.log",
    "ctox.source.rollback_snapshot",
    "ctox.source.save",
    "ctox.subscription_auth.start",
    "ctox.task.delete",
    "ctox.task.update",
    "ctox.workjet.computer.assign",
    "ctox.workjet.computer.endpoint.disable",
    "ctox.workjet.computer.endpoint.list",
    "ctox.workjet.computer.endpoint.upsert",
    "ctox.workjet.computer.list",
    "ctox.workjet.computer.ssh_key.ensure",
    "ctox.workjet.computer.unassign",
    "ctox.workjet.exit_model.read",
    "ctox.workjet.exit_model.refresh",
    "ctox.workjet.exit_model.submit",
    "ctox.workjet.jour_fixe.comment.add",
    "ctox.workjet.jour_fixe.deck.publish",
    "ctox.workjet.jour_fixe.meeting.end",
    "ctox.workjet.jour_fixe.meeting.read",
    "ctox.workjet.jour_fixe.meeting.start",
    "ctox.workjet.jour_fixe.narration.local_publish",
    "ctox.workjet.jour_fixe.prepare",
    "ctox.workjet.jour_fixe.todos.confirm",
    "ctox.workjet.jour_fixe.todos.propose",
    "ctox.workjet.jour_fixe.todos.revise",
    "ctox.workjet.jour_fixe.transcript.append",
    "ctox.workjet.jour_fixe.transcript.local_candidate",
    "ctox.workjet.presentation.canvas.save",
    "ctox.workjet.presentation.edits.apply",
    "ctox.workjet.presentation.read",
    "ctox.workjet.project.chat.create",
    "ctox.workjet.project.chat.ensure",
    "ctox.workjet.project.init",
    "ctox.workjet.project.kpis.configure",
    "ctox.workjet.project.kpis.read",
    "ctox.workjet.project.kpis.refresh",
    "ctox.workjet.project.list",
    "ctox.workjet.project.persistent_worker.bind",
    "ctox.workjet.project.persistent_worker.revoke",
    "ctox.workjet.project.supervisor.bind",
    "ctox.workjet.project.supervisor.route.capabilities.v1",
    "ctox.workjet.project.supervisor.route.capabilities.v2",
    "ctox.workjet.project.supervisor.route.read.v1",
    "ctox.workjet.project.supervisor.route.read.v2",
    "ctox.workjet.project.supervisor.turn.cancel",
    "ctox.workjet.project.supervisor.turn.capabilities",
    "ctox.workjet.project.supervisor.turn.history",
    "ctox.workjet.project.supervisor.turn.input",
    "ctox.workjet.project.supervisor.turn.submit",
    "ctox.workjet.project.supervisor.turn.watch",
    "ctox.workjet.project.upsert",
    "ctox.workjet.project.worker.add",
    "ctox.workjet.project.worker.remove",
    "ctox.workjet.providers.account.enable",
    "ctox.workjet.providers.account.remove",
    "ctox.workjet.providers.adopt_native",
    "ctox.workjet.providers.list",
    "ctox.workjet.providers.models.exclude",
    "ctox.workjet.providers.models.select",
    "ctox.workjet.providers.observe_native",
    "ctox.workjet.providers.withdraw",
    "ctox.workjet.session.create",
    "ctox.workjet.session.delete",
    "ctox.workjet.session.list",
    "ctox.workjet.session.transfer.abort",
    "ctox.workjet.session.transfer.apply_complete",
    "ctox.workjet.session.transfer.confirm_working_copy",
    "ctox.workjet.session.transfer.pack_complete",
    "ctox.workjet.session.transfer.pause_ack",
    "ctox.workjet.session.transfer.resume_ack",
    "ctox.workjet.session.transfer.start",
    "ctox.workjet.session.transfer.status",
    "ctox.workjet.worker_profile.bind",
    "ctox.workjet.worker_profile.unbind",
    "ctox.workjet.working_copy.upsert",
    "knowledge.command",
    "news.source.auth_assist",
    "news.topic.schedule",
    "outbound.lead.research_writeback",
    "web_stack.person_research"
  ],
  "predicate_control_types": [
    "ats.consent.check",
    "ats.credential.verify",
    "ats.deployment.check",
    "ats.intake.capture",
    "ats.interview.transcribe",
    "ats.leistungsnachweis.signoff",
    "ats.placement.create",
    "ats.placement.early_leave",
    "ats.retention.due",
    "ats.retention.purge",
    "ats.signature.request",
    "ats.signature.sign",
    "ats.subject.erase",
    "ats.subject.export",
    "ats.submission.present",
    "ctox.iot.agent.configure",
    "ctox.iot.alarm.update",
    "ctox.iot.asset.delete",
    "ctox.iot.asset.upsert",
    "ctox.iot.attribute.write",
    "ctox.iot.datapoints.query",
    "ctox.iot.ruleset.save",
    "ctox.iot.ruleset.toggle",
    "customers.account.archive",
    "customers.account.create",
    "customers.account.update",
    "customers.activity.record",
    "customers.contact.archive",
    "customers.contact.create",
    "customers.contact.update",
    "customers.dedupe.resolve",
    "customers.import.from_outbound",
    "customers.note.create",
    "customers.note.update",
    "customers.opportunity.close_lost",
    "customers.opportunity.close_won",
    "customers.opportunity.create",
    "customers.opportunity.move_stage",
    "customers.opportunity.update",
    "customers.task.complete",
    "customers.task.create",
    "customers.task.update",
    "customers.view.save",
    "outbound.audit.export",
    "outbound.campaign.apply_setup",
    "outbound.campaign.briefing.update",
    "outbound.campaign.mailbox.link",
    "outbound.campaign.status.set",
    "outbound.dev.seed_test_data",
    "outbound.draft.prepare",
    "outbound.engagement.assign_sender",
    "outbound.engagement.close",
    "outbound.engagement.create",
    "outbound.engagement.reapply_sequence",
    "outbound.engagement.resume",
    "outbound.letter_template.save",
    "outbound.message.approve",
    "outbound.message.cancel",
    "outbound.message.pause",
    "outbound.message.prepare",
    "outbound.message.reject",
    "outbound.message.request_approval",
    "outbound.message.request_changes",
    "outbound.message.resume",
    "outbound.message.send_approved",
    "outbound.message.update_draft",
    "outbound.pipeline.write_outreach_draft",
    "outbound.provider.reconcile",
    "outbound.reply.classify",
    "outbound.reply.match",
    "outbound.research_policy.publish",
    "outbound.research_source.auth_assist",
    "outbound.research_source.generate_adapter",
    "outbound.research_source.registry_read",
    "outbound.research_source.reprobe",
    "outbound.research_source.test",
    "outbound.research_source.upsert",
    "outbound.scheduler.tick",
    "outbound.scheduling.mark_booked",
    "outbound.scheduling.prepare",
    "outbound.scheduling.update_slots",
    "outbound.sellify.lookup",
    "outbound.sequence.save",
    "outbound.skillbook.save",
    "outbound.skillbook.seed_defaults",
    "outbound.update_digest.send_now",
    "support.agent.apply_suggestion",
    "support.agent.reject_suggestion",
    "support.agent.writeback",
    "support.assignment_policy.upsert",
    "support.automation.evaluate",
    "support.automation_rule.upsert",
    "support.bulk.assign",
    "support.bulk.priority",
    "support.bulk.resolve",
    "support.bulk.snooze",
    "support.bulk.status",
    "support.conversation.assign",
    "support.conversation.claim",
    "support.conversation.open_from_thread",
    "support.conversation.priority",
    "support.conversation.reopen",
    "support.conversation.resolve",
    "support.conversation.snooze",
    "support.conversation.status",
    "support.identity.link",
    "support.inbox.upsert",
    "support.macro.run",
    "support.macro.upsert",
    "support.note.create",
    "support.reply.draft",
    "support.reply.send",
    "support.reporting.rebuild_rollups",
    "support.sla.apply",
    "support.sla.recalculate",
    "support.sla_policy.upsert",
    "support.ticket.create_from_conversation",
    "support.ticket.link",
    "support.view.upsert",
    "support.view_filter.upsert",
    "threads.ai.request",
    "threads.ctox_approval.approve",
    "threads.ctox_approval.cancel",
    "threads.ctox_approval.edit",
    "threads.ctox_approval.reject",
    "threads.ctox_approval.request",
    "threads.handoff.create",
    "threads.link.create",
    "threads.link.remove",
    "threads.message.create",
    "threads.note.create",
    "threads.note.delete",
    "threads.note.update",
    "threads.notification.dismiss",
    "threads.notification.mark_read",
    "threads.notification.preferences.update",
    "threads.thread.archive",
    "threads.thread.assign",
    "threads.thread.claim",
    "threads.thread.snooze",
    "threads.thread.state.update",
    "threads.thread.unwatch",
    "threads.thread.watch"
  ],
  "control_prefixes": [
    "ctox.appsec.",
    "ctox.channel.",
    "ctox.ticket.",
    "invoices.",
    "office.document.",
    "office.spreadsheet."
  ],
  "browser_runtime_types": [
    "browser.back",
    "browser.clipboard.clear",
    "browser.clipboard.copy",
    "browser.clipboard.paste",
    "browser.controller.acquire",
    "browser.controller.release",
    "browser.controller.renew",
    "browser.credential.fill",
    "browser.dialog.respond",
    "browser.forward",
    "browser.navigate",
    "browser.permission.respond",
    "browser.reload",
    "browser.reset",
    "browser.session.start",
    "browser.session.stop",
    "browser.tab.activate",
    "browser.tab.close",
    "browser.tab.open",
    "web_stack.auth_assist.complete"
  ],
  "browser_literal_types": [
    "ats.consent.check",
    "ats.deployment.check",
    "ats.intake.capture",
    "ats.leistungsnachweis.signoff",
    "ats.placement.create",
    "ats.placement.early_leave",
    "ats.signature.request",
    "ats.signature.sign",
    "ats.submission.present",
    "browser.capture.extract",
    "browser.credential.fill",
    "business_os.chat.task",
    "business_os.i18n.translate",
    "ctox.app.action.run",
    "ctox.app_store.install",
    "ctox.app_store.uninstall",
    "ctox.browser_context.capture",
    "ctox.business_os.app.create",
    "ctox.business_os.app.modify",
    "ctox.business_os.audit.list",
    "ctox.business_os.branding.update",
    "ctox.business_os.support.export_diagnostics",
    "ctox.business_os.sync.configure",
    "ctox.business_os.user.upsert",
    "ctox.business_os.why",
    "ctox.channel.disconnect",
    "ctox.channel.jami.create",
    "ctox.channel.jami.export",
    "ctox.channel.pair.start",
    "ctox.channel.settings.save",
    "ctox.channel.sync",
    "ctox.channel.test",
    "ctox.coding.models",
    "ctox.coding.turn",
    "ctox.command.cancel",
    "ctox.communication_policy.verify",
    "ctox.crew.assign",
    "ctox.crew.member.create",
    "ctox.crew.member.update",
    "ctox.crew.memory.update",
    "ctox.cv_print.apply_parse",
    "ctox.file.materialize",
    "ctox.knowledge.book.create",
    "ctox.knowledge.book.export",
    "ctox.knowledge.book.import",
    "ctox.knowledge.document.modify",
    "ctox.knowledge.runbook.execute",
    "ctox.knowledge.runbook.modify",
    "ctox.knowledge.runbooks.manage",
    "ctox.mailserver.delete_domain",
    "ctox.mailserver.delete_user",
    "ctox.mailserver.get_config",
    "ctox.mailserver.save_domain",
    "ctox.mailserver.save_runtime",
    "ctox.mailserver.save_user",
    "ctox.maintenance.client_ready",
    "ctox.module.assign_founder",
    "ctox.module.check_updates",
    "ctox.module.delete",
    "ctox.module.install_template",
    "ctox.module.list_versions",
    "ctox.module.release",
    "ctox.module.rollback",
    "ctox.module.rollback_version",
    "ctox.module.save",
    "ctox.module.update",
    "ctox.queue.block",
    "ctox.queue.capacity",
    "ctox.queue.pause",
    "ctox.queue.release",
    "ctox.queue.retry",
    "ctox.runtime.switch",
    "ctox.runtime_settings.save",
    "ctox.secret.delete",
    "ctox.secret.list",
    "ctox.secret.put",
    "ctox.source.rollback_snapshot",
    "ctox.subscription_auth.start",
    "ctox.task.delete",
    "ctox.task.update",
    "ctox.workjet.computer.assign",
    "ctox.workjet.computer.list",
    "ctox.workjet.computer.ssh_key.ensure",
    "ctox.workjet.computer.unassign",
    "ctox.workjet.project.init",
    "ctox.workjet.project.list",
    "ctox.workjet.project.supervisor.bind",
    "ctox.workjet.project.supervisor.turn.history",
    "ctox.workjet.project.upsert",
    "ctox.workjet.working_copy.upsert",
    "customers.account.archive",
    "customers.account.create",
    "customers.account.update",
    "customers.contact.archive",
    "customers.contact.create",
    "customers.contact.update",
    "customers.dedupe.resolve",
    "customers.import.file",
    "customers.import.from_outbound",
    "customers.note.create",
    "customers.note.update",
    "customers.opportunity.create",
    "customers.opportunity.move_stage",
    "customers.opportunity.update",
    "customers.task.complete",
    "customers.task.create",
    "customers.task.update",
    "customers.view.save",
    "document.extract-requirements",
    "document.review-risks",
    "document.summarize",
    "example.control.refresh",
    "example.work.execute",
    "external_sql.write",
    "invoices.dunning.run",
    "invoices.invoice.create",
    "invoices.invoice.delete",
    "invoices.invoice.post",
    "invoices.invoice.update",
    "invoices.payment.allocate",
    "knowledge.command",
    "knowledge.entry.delete",
    "mail.series_email.import",
    "matching.match",
    "matching.source.parse_object",
    "matching.source.parse_requirement",
    "native.sellify_lookup",
    "news.issue.generate",
    "news.source.adapter",
    "news.source.auth_assist",
    "news.topic.schedule",
    "outbound-lead-generation.import",
    "outbound.campaign.apply_setup",
    "outbound.campaign.briefing.update",
    "outbound.company.research",
    "outbound.engagement.create",
    "outbound.lead.research_writeback",
    "outbound.message.prepare",
    "outbound.message.request_approval",
    "outbound.pipeline.contact_research",
    "outbound.pipeline.lead_qualification",
    "outbound.pipeline.outreach_draft",
    "outbound.pipeline.write_outreach_draft",
    "outbound.research.adapters.reconcile",
    "outbound.research_policy.publish",
    "outbound.research_source.registry_read",
    "outbound.sellify.lookup",
    "outbound.source.import",
    "outbound.update_digest.send_now",
    "research.knowledge.refresh",
    "research.systematic.report.create",
    "research.systematic.run",
    "spreadsheet.audit-formulas",
    "spreadsheet.risk-review",
    "spreadsheet.summarize",
    "support.agent.writeback",
    "support.conversation.assign",
    "support.conversation.open_from_thread",
    "support.conversation.snooze",
    "support.note.create",
    "support.ticket.create_from_conversation",
    "threads.ctox_approval.request",
    "web_stack.auth_assist.complete",
    "web_stack.auth_assist.request",
    "web_stack.person_research"
  ]
};

const platformTypes = new Set(['exact_control_types','predicate_control_types','browser_runtime_types','browser_literal_types'].flatMap(k=>COMMAND_INVENTORY[k]));
const chatTypes = new Set(['business_os.chat.task','business_os.context.ask','business_os.data.modify']);
function isPlatformCommand(type) {
 if(typeof type!=='string')return false;
 return platformTypes.has(type) || COMMAND_INVENTORY.control_prefixes.some(p=>type.startsWith(p)) || chatTypes.has(type) || type.startsWith('ctox.app.') || type.startsWith('ctox.coding.') || /^ctox\.workjet\..+\.v1$/.test(type) || type==='ctox.plan.step.retry';
}
function commandTarget(body, manifest) {
 if (isPlatformCommand(body.type)) return {platform:true,target:'ctox-platform'};
 const choices=[...new Set((manifest?.commands_out??[]).filter(x=>x.type===body.type).map(x=>x.app))];
 if (body.target_app!==undefined && !choices.includes(body.target_app)) return {error:'forbidden'};
 if (choices.length>1 && body.target_app===undefined) return {error:'forbidden'};
 return {platform:false,target:body.target_app??choices[0]??body.app_id};
}
function commandRoute(body, manifestOf, user, testRoute=false) {
 const route=commandTarget(body,manifestOf(body.app_id));
 if(route.error || route.platform)return route;
 // An isolated fake Core without a user registry represents the admin bootstrap identity.
 user??={id:body.actor_id??'admin',role:'admin',permissions:[]};
 const target=manifestOf(route.target),cross=route.target!==body.app_id;
 if(cross && !(target?.commands_in??[]).some(x=>x.type===body.type && (x.from==='*'||(Array.isArray(x.from)&&(x.from.includes(body.app_id)||x.from.includes('*'))))))return {error:'forbidden'};
 if(!target?.handlers?.[body.type] && !(testRoute && !cross))return {error:cross?'forbidden':'unknown_command'};
 const rights=target?.command_rights??{};
 // Same precedence as native ManifestRules::authorize_command: exact, then longest prefix.
 const right=rights[body.type]??Object.entries(rights).filter(([k])=>k.endsWith('*')&&body.type.startsWith(k.slice(0,-1))).sort((a,b)=>b[0].length-a[0].length)[0]?.[1];
 if(user && right && !(Array.isArray(right.roles)&&right.roles.includes(user.role)) && !(right.assigned_users && user.assigned_apps?.includes(route.target)))return {error:'forbidden'};
 // The simulator's canonical collection write rules mirror its Core rights facade. No invented permission token.
 const writable = Object.values(target?.collections??{}).some(c=>String(c.rights?.write??'all').split('+').every(token=>token!=='core'&&(token==='all'||token==='*'||token==='own'||token===`app:${route.target}`||token===`role:${user?.role}`||(token==='admin'&&user?.role==='admin')||(user?.permissions??[]).includes(token))));
 if(cross && user && user.role!=='admin' && !right && !writable)return {error:'forbidden'};
 return route;
}

// E26.5: observational, per-tab, bounded by command creation order.
// No command payloads, durable state or authority are kept in diagnostics.
function createCommandTimings(changed) {
  const rows = new Map();
  const stages = ['queued', 'sent', 'acked', 'completed'];
  function merge(value, broadcast = false) {
    if (!value || typeof value.command_id !== 'string') return;
    let row = rows.get(value.command_id);
    if (!row) {
      if (!Number.isFinite(value.queued)) return; // late events cannot resurrect an evicted command
      row = { command_id: value.command_id, queued: value.queued, sent: null, acked: null, completed: null };
      rows.set(value.command_id, row);
      if (rows.size > 500) rows.delete(rows.keys().next().value);
    }
    let updated = !row.published;
    for (const stage of stages) {
      if (row[stage] == null && Number.isFinite(value[stage])) { row[stage] = value[stage]; updated = true; }
    }
    if (!updated) return;
    row.published = true;
    const { published, ...snapshot } = row;
    changed(Object.freeze(snapshot), broadcast, value);
  }
  return {
    mark(command_id, stage, at = Date.now()) {
      if (!stages.includes(stage)) throw new TypeError('Unknown command timing stage');
      merge({ command_id, [stage]: at }, true);
    },
    merge,
    snapshot: () => [...rows.values()].map(({ published, ...row }) => ({ ...row })),
  };
}

// CTOX App-API 4.0 – ausführbarer Stub (Browser-ES-Modul, keine Abhängigkeiten, kein Build, kein Netz).
// Vertrag: ctox-sync-v4-architektur.md §10.2, §10.2a, §6, §7.5, §9.3, §16.4, §16.6 (Teile), §16.7, §16.10.
// Alles, was „Core“ heißt, ist hier simuliert (kernel.sim). Entscheidungen zu Vertragslücken: README.md.

export const APP_API_VERSION = '4.0';
const IDB_VERSION = 5;
const STORES = {
  docs: ['collection', 'id'],
  meta: 'key',
  outbox: 'op_id',
  artifacts: 'hash',
  archive_local: 'ref',
  window_state: 'key',
  blobs: 'ref', // Stub-Entscheidung: lokaler Datei-Blob-Cache (§10.2a Nr. 4), getrennt von App-Artefakten
  sim_core: 'key', // nur Simulation: Zustand des simulierten Core (seq, _ops, CAS, Archiv)
};
const STORE_NAMES = Object.keys(STORES);
const SNAPSHOT_MAX_BYTES = 1024 * 1024;
const SYSTEM_COLLECTIONS = new Set(['business_commands', 'files', 'business_apps', 'business_app_versions', 'shell_preferences', 'app_drafts']);
const LAYOUT_KEYS = ['icon_asset', 'icon_svg', 'category', 'frame_palette', 'default_width', 'default_height', 'min_width', 'min_height', 'presentation'];
// Tab-Identität für window_state: überlebt Neuladen desselben Tabs (sessionStorage), sonst je Kernel neu.
function sessionTabKey() {
  try {
    const ss = globalThis.sessionStorage;
    if (ss) {
      let k = ss.getItem('ctox-tab-key');
      if (!k) { k = uuidv7(); ss.setItem('ctox-tab-key', k); }
      return k;
    }
  } catch { /* privater Modus o. ä. */ }
  return uuidv7();
}

// ---------------------------------------------------------------------------------------------------------------
// Fehler
// ---------------------------------------------------------------------------------------------------------------
export const CTOX_ERROR_CODES = ['revision_conflict', 'invalid', 'forbidden', 'not_found', 'conflict', 'revoked', 'expired', 'quota', 'unavailable'];
export class CtoxError extends Error {
  constructor(code, message, extra = {}) {
    super(message || code);
    this.name = 'CtoxError';
    this.code = CTOX_ERROR_CODES.includes(code) ? code : 'invalid';
    if (extra.path !== undefined) this.path = extra.path;
    if (extra.reason !== undefined) this.reason = extra.reason;
  }
}
class StaleLeader extends Error {}

// ---------------------------------------------------------------------------------------------------------------
// Hilfsfunktionen
// ---------------------------------------------------------------------------------------------------------------
const hasBlob = typeof Blob !== 'undefined';
const isBlob = (v) => hasBlob && v instanceof Blob;
const isPlainObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v) && !(v instanceof RegExp) && !isBlob(v) && !(v instanceof Date) && !ArrayBuffer.isView(v);

export function cloneValue(v) {
  if (v === null || typeof v !== 'object') return v;
  if (isBlob(v) || v instanceof RegExp) return v;
  if (v instanceof Date) return new Date(v.getTime());
  if (ArrayBuffer.isView(v)) return v.slice();
  if (Array.isArray(v)) return v.map(cloneValue);
  const o = {};
  for (const k of Object.keys(v)) o[k] = cloneValue(v[k]);
  return o;
}
function deepFreeze(v) {
  if (v === null || typeof v !== 'object' || Object.isFrozen(v) || isBlob(v) || ArrayBuffer.isView(v)) return v;
  Object.freeze(v);
  for (const k of Object.keys(v)) deepFreeze(v[k]);
  return v;
}
export function deepEqual(a, b) {
  if (a === b) return true;
  if (a === null || b === null || typeof a !== 'object' || typeof b !== 'object') return Number.isNaN(a) && Number.isNaN(b);
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) return a.length === b.length && a.every((x, i) => deepEqual(x, b[i]));
  const ka = Object.keys(a), kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && deepEqual(a[k], b[k]));
}
function jsonEq(a, b) {
  if (a === b) return true;
  try { return JSON.stringify(a) === JSON.stringify(b); } catch { return false; }
}
function liveEqual(a, b) {
  if (a === b) return true;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((x, i) => x === b[i] || jsonEq(x, b[i]));
  return jsonEq(a, b);
}
// RFC 7396 JSON-Merge-Patch, mutiert nichts.
export function mergePatch(target, patch) {
  if (!isPlainObject(patch)) return cloneValue(patch);
  const out = isPlainObject(target) ? { ...target } : {};
  for (const [k, v] of Object.entries(patch)) {
    if (v === null) delete out[k];
    else out[k] = mergePatch(out[k], v);
  }
  return out;
}
function fnv1a(str) {
  let h = 0x811c9dc5;
  for (let i = 0; i < str.length; i++) { h ^= str.charCodeAt(i); h = Math.imul(h, 0x01000193) >>> 0; }
  return h.toString(16).padStart(8, '0');
}
function randomBytes(n) {
  const b = new Uint8Array(n);
  if (globalThis.crypto?.getRandomValues) globalThis.crypto.getRandomValues(b);
  else for (let i = 0; i < n; i++) b[i] = Math.floor(Math.random() * 256);
  return b;
}
let uuidLastMs = 0, uuidCounter = 0;
// UUIDv7 (RFC 9562), monoton je Realm: 48 Bit ms, 12 Bit Zähler, 62 Bit Zufall. Lexikografisch = zeitlich sortiert.
export function uuidv7() {
  let ms = Date.now();
  if (ms <= uuidLastMs) {
    ms = uuidLastMs;
    uuidCounter++;
    if (uuidCounter > 0xfff) { ms = ++uuidLastMs; uuidCounter = 0; }
  } else { uuidLastMs = ms; uuidCounter = 0; }
  const b = randomBytes(16);
  for (let i = 5; i >= 0; i--) { b[i] = ms % 256; ms = Math.floor(ms / 256); }
  b[6] = 0x70 | ((uuidCounter >> 8) & 0x0f);
  b[7] = uuidCounter & 0xff;
  b[8] = 0x80 | (b[8] & 0x3f);
  const h = Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

// SHA-256: crypto.subtle, sonst reines JS (z. B. unsichere Kontexte).
const SHA_K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
  0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
  0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
  0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
  0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
  0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);
export function sha256Js(bytes) {
  const len = bytes.length;
  const total = ((len + 9 + 63) >> 6) << 6;
  const buf = new Uint8Array(total);
  buf.set(bytes);
  buf[len] = 0x80;
  const dv = new DataView(buf.buffer);
  dv.setUint32(total - 4, (len * 8) >>> 0);
  dv.setUint32(total - 8, Math.floor((len * 8) / 0x100000000));
  const H = new Uint32Array([0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]);
  const w = new Uint32Array(64);
  const ror = (x, n) => (x >>> n) | (x << (32 - n));
  for (let off = 0; off < total; off += 64) {
    for (let i = 0; i < 16; i++) w[i] = dv.getUint32(off + i * 4);
    for (let i = 16; i < 64; i++) {
      const a = w[i - 15], b = w[i - 2];
      w[i] = (w[i - 16] + (ror(a, 7) ^ ror(a, 18) ^ (a >>> 3)) + w[i - 7] + (ror(b, 17) ^ ror(b, 19) ^ (b >>> 10))) >>> 0;
    }
    let [a, b, c, d, e, f, g, h] = H;
    for (let i = 0; i < 64; i++) {
      const t1 = (h + (ror(e, 6) ^ ror(e, 11) ^ ror(e, 25)) + ((e & f) ^ (~e & g)) + SHA_K[i] + w[i]) >>> 0;
      const t2 = ((ror(a, 2) ^ ror(a, 13) ^ ror(a, 22)) + ((a & b) ^ (a & c) ^ (b & c))) >>> 0;
      h = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0;
    }
    H[0] += a; H[1] += b; H[2] += c; H[3] += d; H[4] += e; H[5] += f; H[6] += g; H[7] += h;
  }
  return Array.from(H, (x) => x.toString(16).padStart(8, '0')).join('');
}
async function toBytes(content) {
  if (typeof content === 'string') return new TextEncoder().encode(content);
  if (isBlob(content)) return new Uint8Array(await content.arrayBuffer());
  if (content instanceof ArrayBuffer) return new Uint8Array(content);
  if (ArrayBuffer.isView(content)) return new Uint8Array(content.buffer, content.byteOffset, content.byteLength);
  throw new CtoxError('invalid', 'Inhalt muss string, Blob oder Bytes sein');
}
export async function sha256Hex(content) {
  const bytes = await toBytes(content);
  if (globalThis.crypto?.subtle?.digest) {
    try {
      const d = await globalThis.crypto.subtle.digest('SHA-256', bytes);
      return Array.from(new Uint8Array(d), (x) => x.toString(16).padStart(2, '0')).join('');
    } catch { /* unsicherer Kontext → JS */ }
  }
  return sha256Js(bytes);
}

// ---------------------------------------------------------------------------------------------------------------
// Mango-Teilmenge (§10.2a Nr. 5, §16.10): {selector, sort?, limit?, skip?}
// Fehlendes Feld ≠ null; $exists:false trifft fehlende Felder; $eq:null trifft nur null.
// ---------------------------------------------------------------------------------------------------------------
const FIELD_OPS = new Set(['$eq', '$ne', '$gt', '$gte', '$lt', '$lte', '$in', '$nin', '$exists', '$regex', '$options', '$not', '$elemMatch']);
const isOpObject = (c) => isPlainObject(c) && Object.keys(c).length > 0 && Object.keys(c).every((k) => k.startsWith('$'));

function validateSelector(sel, path, inElem) {
  if (!isPlainObject(sel)) throw new CtoxError('invalid', `Selektor muss ein Objekt sein (${path})`, { path });
  for (const [k, v] of Object.entries(sel)) {
    if (k === '$and' || k === '$or') {
      if (!Array.isArray(v) || !v.length) throw new CtoxError('invalid', `${k} braucht ein nichtleeres Array`, { path: `${path}.${k}` });
      v.forEach((s, i) => validateSelector(s, `${path}.${k}[${i}]`, inElem));
    } else if (k === '$not') validateSelector(v, `${path}.$not`, inElem);
    else if (k.startsWith('$')) throw new CtoxError('invalid', `Unbekannter Operator ${k}`, { path: `${path}.${k}` });
    else validateCond(v, `${path}.${k}`, inElem);
  }
}
function validateCond(c, path, inElem) {
  if (c instanceof RegExp) return validateRegex(c, path);
  if (!isOpObject(c)) return;
  for (const [op, arg] of Object.entries(c)) {
    if (!FIELD_OPS.has(op)) throw new CtoxError('invalid', `Unbekannter Operator ${op}`, { path: `${path}.${op}` });
    if ((op === '$in' || op === '$nin') && !Array.isArray(arg)) throw new CtoxError('invalid', `${op} braucht ein Array`, { path: `${path}.${op}` });
    if (op === '$exists' && typeof arg !== 'boolean') throw new CtoxError('invalid', '$exists braucht true|false', { path: `${path}.${op}` });
    if (op === '$regex') {
      if (arg instanceof RegExp) validateRegex(arg, path);
      else if (typeof arg !== 'string') throw new CtoxError('invalid', '$regex braucht string oder RegExp', { path: `${path}.${op}` });
      if (c.$options !== undefined && !/^i?$/.test(c.$options)) throw new CtoxError('invalid', 'Nur Flag i erlaubt', { path: `${path}.$options` });
    }
    if (op === '$options' && c.$regex === undefined) throw new CtoxError('invalid', '$options nur mit $regex', { path: `${path}.${op}` });
    if (op === '$not') validateCond(arg, `${path}.$not`, inElem);
    if (op === '$elemMatch') {
      if (inElem) throw new CtoxError('invalid', '$elemMatch nur eine Ebene', { path: `${path}.${op}` });
      if (!isPlainObject(arg)) throw new CtoxError('invalid', '$elemMatch braucht ein Objekt', { path: `${path}.${op}` });
      if (isOpObject(arg)) validateCond(arg, `${path}.$elemMatch`, true);
      else validateSelector(arg, `${path}.$elemMatch`, true);
    }
  }
}
function validateRegex(re, path) {
  if (re.flags.replace('i', '') !== '') throw new CtoxError('invalid', 'Nur Regex-Flag i erlaubt', { path });
}
function getPath(doc, path) {
  let cur = doc;
  for (const part of path.split('.')) {
    if (cur === null || typeof cur !== 'object' || !Object.prototype.hasOwnProperty.call(cur, part)) return undefined;
    cur = cur[part];
  }
  return cur;
}
function comparable(a, b) {
  return (typeof a === 'number' && typeof b === 'number') || (typeof a === 'string' && typeof b === 'string');
}
function matchSelector(doc, sel) {
  for (const [k, cond] of Object.entries(sel)) {
    if (k === '$and') { if (!cond.every((s) => matchSelector(doc, s))) return false; }
    else if (k === '$or') { if (!cond.some((s) => matchSelector(doc, s))) return false; }
    else if (k === '$not') { if (matchSelector(doc, cond)) return false; }
    else if (!matchCond(getPath(doc, k), cond)) return false;
  }
  return true;
}
function matchCond(v, cond) {
  if (cond instanceof RegExp) return typeof v === 'string' && cond.test(v);
  if (!isOpObject(cond)) return v !== undefined && deepEqual(v, cond);
  for (const [op, arg] of Object.entries(cond)) {
    switch (op) {
      case '$eq': if (v === undefined || !deepEqual(v, arg)) return false; break;
      case '$ne': if (v !== undefined && deepEqual(v, arg)) return false; break;
      case '$gt': if (!(comparable(v, arg) && v > arg)) return false; break;
      case '$gte': if (!(comparable(v, arg) && v >= arg)) return false; break;
      case '$lt': if (!(comparable(v, arg) && v < arg)) return false; break;
      case '$lte': if (!(comparable(v, arg) && v <= arg)) return false; break;
      case '$in': if (v === undefined || !arg.some((a) => deepEqual(v, a))) return false; break;
      case '$nin': if (v !== undefined && arg.some((a) => deepEqual(v, a))) return false; break;
      case '$exists': if ((v !== undefined) !== arg) return false; break;
      case '$regex': {
        const re = arg instanceof RegExp ? arg : new RegExp(arg, cond.$options === 'i' ? 'i' : '');
        if (typeof v !== 'string' || !re.test(v)) return false;
        break;
      }
      case '$options': break;
      case '$not': if (matchCond(v, arg)) return false; break;
      case '$elemMatch':
        if (!Array.isArray(v) || !v.some((el) => (isOpObject(arg) ? matchCond(el, arg) : isPlainObject(el) && matchSelector(el, arg)))) return false;
        break;
      default: return false;
    }
  }
  return true;
}
// Typordnung für sort: fehlend < null < Zahl < String < Boolean < Rest; Gleichstand nach id.
function typeRank(v) {
  if (v === undefined) return 0;
  if (v === null) return 1;
  if (typeof v === 'number') return 2;
  if (typeof v === 'string') return 3;
  if (typeof v === 'boolean') return 4;
  return 5;
}
function compareValues(a, b) {
  const ra = typeRank(a), rb = typeRank(b);
  if (ra !== rb) return ra - rb;
  if (ra === 2 || ra === 3) return a < b ? -1 : a > b ? 1 : 0;
  if (ra === 4) return a === b ? 0 : a ? 1 : -1;
  if (ra === 5) { const sa = JSON.stringify(a), sb = JSON.stringify(b); return sa < sb ? -1 : sa > sb ? 1 : 0; }
  return 0;
}
export function compileQuery(q = {}) {
  if (!isPlainObject(q)) throw new CtoxError('invalid', 'Query muss ein Objekt sein');
  for (const k of Object.keys(q)) if (!['selector', 'sort', 'limit', 'skip'].includes(k)) throw new CtoxError('invalid', `Unbekannter Query-Schlüssel ${k}`, { path: k });
  const selector = q.selector ?? {};
  validateSelector(selector, 'selector', false);
  const sort = q.sort ?? [];
  if (!Array.isArray(sort)) throw new CtoxError('invalid', 'sort muss eine Liste [{feld: asc|desc}] sein', { path: 'sort' });
  const sortSpec = sort.map((s, i) => {
    if (!isPlainObject(s) || Object.keys(s).length !== 1) throw new CtoxError('invalid', 'sort-Eintrag braucht genau ein Feld', { path: `sort[${i}]` });
    const [field, dir] = Object.entries(s)[0];
    if (dir !== 'asc' && dir !== 'desc') throw new CtoxError('invalid', 'Richtung asc|desc', { path: `sort[${i}].${field}` });
    return [field, dir === 'desc' ? -1 : 1];
  });
  for (const k of ['limit', 'skip']) {
    if (q[k] !== undefined && !(Number.isInteger(q[k]) && q[k] >= 0)) throw new CtoxError('invalid', `${k} muss ganzzahlig ≥ 0 sein`, { path: k });
  }
  return { selector, sortSpec, limit: q.limit, skip: q.skip ?? 0 };
}
export function runCompiledQuery(docs, cq) {
  const out = [];
  for (const d of docs) if (matchSelector(d, cq.selector)) out.push(d);
  out.sort((a, b) => {
    for (const [f, dir] of cq.sortSpec) {
      const c = compareValues(getPath(a, f), getPath(b, f));
      if (c) return c * dir;
    }
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
  });
  const start = cq.skip;
  return cq.limit === undefined ? out.slice(start) : out.slice(start, start + cq.limit);
}

// ---------------------------------------------------------------------------------------------------------------
// Lokale Schemaprüfung (§16.10): JSON-Werte + Teilmenge JSON-Schema (type, required, properties, items, enum,
// additionalProperties:false) aus dem Manifest der schreibenden App-Version.
// ---------------------------------------------------------------------------------------------------------------
function normalizeJson(v, path) {
  if (v === null || typeof v === 'string' || typeof v === 'boolean') return v;
  if (typeof v === 'number') {
    if (!Number.isFinite(v)) throw new CtoxError('invalid', 'Zahl nicht endlich', { path });
    return v;
  }
  if (Array.isArray(v)) return v.map((x, i) => normalizeJson(x === undefined ? null : x, `${path}[${i}]`));
  if (isPlainObject(v)) {
    const o = {};
    for (const [k, x] of Object.entries(v)) if (x !== undefined) o[k] = normalizeJson(x, path ? `${path}.${k}` : k);
    return o;
  }
  throw new CtoxError('invalid', 'Kein JSON-Wert (Date/Blob/Funktion nicht erlaubt; Dateien über api.files)', { path });
}
function jsonType(v) {
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'array';
  if (typeof v === 'number') return Number.isInteger(v) ? 'integer' : 'number';
  return typeof v;
}
function validateSchema(v, schema, path) {
  if (!schema || typeof schema !== 'object') return;
  if (schema.type) {
    const types = Array.isArray(schema.type) ? schema.type : [schema.type];
    const t = jsonType(v);
    if (!types.includes(t) && !(t === 'integer' && types.includes('number'))) throw new CtoxError('invalid', `Typ ${t}, erwartet ${types.join('|')}`, { path });
  }
  if (schema.enum && !schema.enum.some((e) => deepEqual(e, v))) throw new CtoxError('invalid', 'Wert nicht in enum', { path });
  if (isPlainObject(v)) {
    for (const r of schema.required ?? []) if (v[r] === undefined) throw new CtoxError('invalid', `Pflichtfeld ${r} fehlt`, { path: path ? `${path}.${r}` : r });
    for (const [k, x] of Object.entries(v)) {
      const sub = schema.properties?.[k];
      if (sub) validateSchema(x, sub, path ? `${path}.${k}` : k);
      else if (schema.additionalProperties === false) throw new CtoxError('invalid', `Feld ${k} nicht erlaubt`, { path: path ? `${path}.${k}` : k });
    }
  }
  if (Array.isArray(v) && schema.items) v.forEach((x, i) => validateSchema(x, schema.items, `${path}[${i}]`));
}

// ---------------------------------------------------------------------------------------------------------------
// collections.schema.json und Schema-Diff (§16.22 A2/A6/A9, Regeln wie Shell v4 source/schema-diff.js, §16.4):
//   kompatibel      = neue Collection, neues OPTIONALES Feld MIT Default, Index, Rechte, Klasse/Limit/Budget
//   neue Generation = alles andere (Typänderung, Feld entfernt/umbenannt, neues Pflichtfeld, Feld ohne Default, …)
//   entfernt        = Collection fehlt im Ziel (Archiv §9.5), keine Migration verlangt
// Format: { collections: { <name>: <Schema> | {schema, indexes, class, rights, client_limit, byte_budget} },
//           migrations:  { <name>: { from_ver, migrate_fn: '<pfad>' } | { from_ver, steps: [...] } } }
// ---------------------------------------------------------------------------------------------------------------
export const SCHEMA_FILE = 'collections.schema.json';
const SCHEMA_META_KEYS = ['class', 'rights', 'client_limit', 'byte_budget'];
function canon(v) {
  if (Array.isArray(v)) return `[${v.map(canon).join(',')}]`;
  if (v && typeof v === 'object') return `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${canon(v[k])}`).join(',')}}`;
  return JSON.stringify(v ?? null);
}
const sameCanon = (a, b) => canon(a) === canon(b);
function normalizeSchemaEntry(entry) {
  const e = isPlainObject(entry) ? entry : {};
  const wrapped = isPlainObject(e.schema);
  const schema = { ...(wrapped ? e.schema : e) };
  if (!wrapped) for (const k of SCHEMA_META_KEYS) delete schema[k];
  const indexes = (wrapped ? e.indexes : undefined) ?? schema.indexes ?? [];
  delete schema.indexes;
  const out = { schema, indexes };
  for (const k of SCHEMA_META_KEYS) out[k] = e[k] ?? null;
  return out;
}
/** Text oder Objekt → { collections: {name: {schema, indexes, class, rights, client_limit, byte_budget}}, migrations }. */
export function parseCollectionsFile(input) {
  let obj;
  try { obj = typeof input === 'string' ? (input.trim() ? JSON.parse(input) : {}) : (input ?? {}); } catch (e) {
    throw new CtoxError('invalid', `${SCHEMA_FILE}: kein JSON (${e.message})`, { path: SCHEMA_FILE });
  }
  if (!isPlainObject(obj)) throw new CtoxError('invalid', `${SCHEMA_FILE}: Objekt erwartet`, { path: SCHEMA_FILE });
  if (obj.collections !== undefined && !isPlainObject(obj.collections)) throw new CtoxError('invalid', `${SCHEMA_FILE}: collections muss ein Objekt sein`, { path: `${SCHEMA_FILE}.collections` });
  if (obj.migrations !== undefined && !isPlainObject(obj.migrations)) throw new CtoxError('invalid', `${SCHEMA_FILE}: migrations muss ein Objekt sein`, { path: `${SCHEMA_FILE}.migrations` });
  const collections = {};
  for (const [name, entry] of Object.entries(obj.collections ?? {})) collections[name] = normalizeSchemaEntry(entry);
  return { collections, migrations: cloneValue(obj.migrations ?? {}) };
}
function manifestCollectionsToParsed(mc) {
  const collections = {};
  for (const [name, c] of Object.entries(mc ?? {})) collections[name] = normalizeSchemaEntry(c);
  return { collections, migrations: {} };
}
function parsedToManifestCollections(parsed) {
  const out = {};
  for (const [name, c] of Object.entries(parsed.collections)) {
    out[name] = { schema: { ...c.schema, ...(c.indexes?.length ? { indexes: c.indexes } : {}) }, class: c.class, rights: c.rights, client_limit: c.client_limit, byte_budget: c.byte_budget };
  }
  return out;
}
const typeLabel = (p) => (!p || typeof p !== 'object' ? '?' : Array.isArray(p.type) ? p.type.join('|') : (p.type ?? (p.enum ? 'enum' : 'any')));
function diffCollection(a, b) {
  const compatible = [], generation = [];
  const pa = a.schema.properties ?? {}, pb = b.schema.properties ?? {};
  const ra = new Set(a.schema.required ?? []), rb = new Set(b.schema.required ?? []);
  for (const [field, def] of Object.entries(pb)) {
    if (!(field in pa)) {
      const optional = !rb.has(field);
      const hasDefault = isPlainObject(def) && 'default' in def;
      if (optional && hasDefault) compatible.push(`Feld „${field}“ neu (optional, Default)`);
      else generation.push(`Feld „${field}“ neu ${optional ? 'ohne Default' : 'als Pflichtfeld'}`);
    } else if (!sameCanon(pa[field], def)) {
      generation.push(typeLabel(pa[field]) !== typeLabel(def) ? `Typ von „${field}“: ${typeLabel(pa[field])} → ${typeLabel(def)}` : `Feld „${field}“ geändert`);
    }
  }
  for (const field of Object.keys(pa)) if (!(field in pb)) generation.push(`Feld „${field}“ entfernt`);
  for (const field of rb) if (!ra.has(field) && field in pa) generation.push(`„${field}“ wird Pflichtfeld`);
  for (const field of ra) if (!rb.has(field) && field in pb) generation.push(`„${field}“ nicht mehr Pflicht`);
  for (const k of new Set([...Object.keys(a.schema), ...Object.keys(b.schema)])) {
    if (k !== 'properties' && k !== 'required' && !sameCanon(a.schema[k], b.schema[k])) generation.push(`Schema-Eigenschaft „${k}“ geändert`);
  }
  if (!sameCanon(a.indexes, b.indexes)) compatible.push('Indizes geändert');
  for (const k of SCHEMA_META_KEYS) if (!sameCanon(a[k], b[k])) compatible.push(k === 'rights' ? 'Rechte geändert' : `${k} geändert`);
  return { compatible, generation };
}
/**
 * Je Collection { name, kind: 'unchanged'|'compatible'|'new'|'generation'|'removed', reasons[], needsMigration, migration }.
 * Eine Migration zählt nur mit from_ver === baseVer (sonst Rest einer früheren Generation) und migrate_fn oder steps.
 */
export function classifyCollections(baseParsed, nextParsed, { baseVer = null } = {}) {
  const a = baseParsed?.collections ?? {}, b = nextParsed?.collections ?? {};
  return [...new Set([...Object.keys(a), ...Object.keys(b)])].sort().map((name) => {
    if (!(name in a)) return { name, kind: 'new', reasons: ['Collection neu'], needsMigration: false, migration: null };
    if (!(name in b)) return { name, kind: 'removed', reasons: ['Collection entfernt (wird archiviert)'], needsMigration: false, migration: null };
    const { compatible, generation } = diffCollection(a[name], b[name]);
    const kind = generation.length ? 'generation' : compatible.length ? 'compatible' : 'unchanged';
    const m = nextParsed.migrations?.[name] ?? null;
    const usable = m && (m.migrate_fn || Array.isArray(m.steps)) && (baseVer == null || String(m.from_ver) === String(baseVer));
    return { name, kind, reasons: [...generation, ...compatible], needsMigration: kind === 'generation', migration: kind === 'generation' && usable ? cloneValue(m) : null };
  });
}
// Deklarative Migrationsschritte (Stub-Teilmenge): rename{from,to} · add{field,value} · remove{field} · cast{field,to}
export function applyMigrationSteps(doc, steps) {
  const d = { ...doc };
  for (const st of steps ?? []) {
    switch (st?.op) {
      case 'rename': if (st.from in d) { d[st.to] = d[st.from]; delete d[st.from]; } break;
      case 'add': if (!(st.field in d)) d[st.field] = cloneValue(st.value ?? null); break;
      case 'remove': delete d[st.field]; break;
      case 'cast': if (st.field in d && d[st.field] != null) {
        d[st.field] = st.to === 'string' ? String(d[st.field]) : st.to === 'number' ? Number(d[st.field]) : st.to === 'boolean' ? Boolean(d[st.field]) : d[st.field];
      } break;
      default: throw new CtoxError('invalid', `Unbekannter Migrationsschritt ${st?.op}`, { path: 'steps' });
    }
  }
  return d;
}
async function importModuleText(text) {
  let url, revoke = false;
  if (typeof window !== 'undefined' && typeof URL.createObjectURL === 'function' && hasBlob) {
    url = URL.createObjectURL(new Blob([text], { type: 'text/javascript' }));
    revoke = true;
  } else url = `data:text/javascript;base64,${btoa(String.fromCharCode(...new TextEncoder().encode(text)))}`;
  try { return await import(/* @vite-ignore */ url); } finally { if (revoke) URL.revokeObjectURL(url); }
}

// ---------------------------------------------------------------------------------------------------------------
// Speicher: IndexedDB oder In-Memory mit gleicher Schnittstelle. tx(fn) ist atomar (IDB: eine readwrite-Transaktion
// über alle Stores; Memory: serialisiert). In fn nur t.* awaiten, nichts anderes.
// ---------------------------------------------------------------------------------------------------------------
const MEMORY_DBS = new Map();
const memKey = (k) => (Array.isArray(k) ? JSON.stringify(k) : String(k));
function keyOfValue(store, v) {
  const kp = STORES[store];
  return Array.isArray(kp) ? memKey(kp.map((p) => v[p])) : memKey(v[kp]);
}
class MemoryBackend {
  constructor(name) {
    this.name = name;
    this.stores = new Map(STORE_NAMES.map((s) => [s, new Map()]));
    this.lock = Promise.resolve();
    this.kind = 'memory';
  }
  _ops() {
    const st = (s) => {
      const m = this.stores.get(s);
      if (!m) throw new Error(`Unbekannter Store ${s}`);
      return m;
    };
    return {
      get: async (s, k) => cloneValue(st(s).get(memKey(k))),
      put: async (s, v) => { st(s).set(keyOfValue(s, v), cloneValue(v)); },
      del: async (s, k) => { st(s).delete(memKey(k)); },
      all: async (s) => [...st(s).values()].map(cloneValue),
      outboxFor: async (ns, id) => [...st('outbox').values()].filter((e) => e.collection === ns && e.id === id).map(cloneValue),
    };
  }
  async outboxStat() {
    let count = 0, oldest = null;
    for (const e of this.stores.get('outbox').values()) { count++; if (oldest === null || e.created_at < oldest) oldest = e.created_at; }
    return { count, oldest_at: oldest };
  }
  tx(fn) {
    const run = this.lock.then(() => fn(this._ops()));
    this.lock = run.catch(() => {});
    return run;
  }
  get(s, k) { return this._ops().get(s, k); }
  all(s) { return this._ops().all(s); }
  close() {}
}
class IdbBackend {
  static open(name) {
    return new Promise((resolve, reject) => {
      const req = indexedDB.open(name, IDB_VERSION);
      req.onupgradeneeded = () => {
        const db = req.result;
        for (const [s, kp] of Object.entries(STORES)) {
          if (db.objectStoreNames.contains(s)) continue;
          const os = db.createObjectStore(s, { keyPath: kp });
          if (s === 'outbox') os.createIndex('by_doc', ['collection', 'id']);
        }
      };
      req.onsuccess = () => resolve(new IdbBackend(req.result, name));
      req.onerror = () => reject(req.error);
      req.onblocked = () => reject(new CtoxError('unavailable', 'IndexedDB-Upgrade blockiert (anderer Tab mit alter Version)'));
    });
  }
  constructor(db, name) {
    this.db = db;
    this.name = name;
    this.kind = 'indexeddb';
    db.onversionchange = () => db.close();
  }
  _wrap(t) {
    const req = (r) => new Promise((res, rej) => { r.onsuccess = () => res(r.result); r.onerror = () => rej(r.error); });
    return {
      get: (s, k) => req(t.objectStore(s).get(k)),
      put: (s, v) => req(t.objectStore(s).put(v)),
      del: (s, k) => req(t.objectStore(s).delete(k)),
      all: (s) => req(t.objectStore(s).getAll()),
      outboxFor: (ns, id) => req(t.objectStore('outbox').index('by_doc').getAll([ns, id])),
    };
  }
  async outboxStat() {
    const all = await this.all('outbox');
    return { count: all.length, oldest_at: all.length ? Math.min(...all.map((e) => e.created_at)) : null };
  }
  tx(fn) {
    return new Promise((resolve, reject) => {
      let t;
      try { t = this.db.transaction(STORE_NAMES, 'readwrite'); } catch (e) { reject(e); return; }
      let out, failed = null;
      t.oncomplete = () => (failed ? reject(failed) : resolve(out));
      t.onabort = () => reject(failed || t.error || new Error('IDB-Transaktion abgebrochen'));
      Promise.resolve().then(() => fn(this._wrap(t))).then(
        (v) => { out = v; },
        (e) => { failed = e; try { t.abort(); } catch { reject(e); } },
      );
    });
  }
  _ro(s, op) {
    return new Promise((resolve, reject) => {
      const t = this.db.transaction([s], 'readonly');
      const r = op(t.objectStore(s));
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error);
    });
  }
  get(s, k) { return this._ro(s, (os) => os.get(k)); }
  all(s) { return this._ro(s, (os) => os.getAll()); }
  close() { this.db.close(); }
}
async function openBackend(name, mode) {
  const useIdb = mode === 'indexeddb' || (mode !== 'memory' && typeof indexedDB !== 'undefined');
  if (useIdb) return IdbBackend.open(name);
  let db = MEMORY_DBS.get(name);
  if (!db) { db = new MemoryBackend(name); MEMORY_DBS.set(name, db); }
  return db;
}
async function deleteBackend(name, kind) {
  if (kind === 'memory') { MEMORY_DBS.delete(name); return; }
  await new Promise((res) => { const r = indexedDB.deleteDatabase(name); r.onsuccess = r.onerror = r.onblocked = () => res(); });
}

// ---------------------------------------------------------------------------------------------------------------
// Kernel
// ---------------------------------------------------------------------------------------------------------------
const emptyBase = () => ({ data: null, deleted: false, rev: null, seq: 0 });
const byOpId = (a, b) => (a.op_id < b.op_id ? -1 : a.op_id > b.op_id ? 1 : 0);
const docKey = (ns, id) => `${ns}\u0000${id}`;
const nsName = (collection, gen) => (gen > 1 ? `${collection}@g${gen}` : collection);

function applyOp(state, entry) {
  if (entry.op === 'put') return { data: cloneValue(entry.body), deleted: false };
  if (entry.op === 'patch') {
    const cur = state.data && !state.deleted ? state.data : { id: entry.id };
    const next = mergePatch(cur, entry.body);
    next.id = entry.id;
    return { data: next, deleted: false };
  }
  if (entry.op === 'delete') return { data: null, deleted: true };
  return state;
}
function recompute(ns, id, base, outboxEntries) {
  let st = { data: base.data, deleted: base.deleted };
  const mine = outboxEntries.filter((e) => e.collection === ns && e.id === id && e.op !== 'upload').sort(byOpId);
  for (const e of mine) st = applyOp(st, e);
  const last = mine[mine.length - 1];
  return {
    collection: ns, id, data: st.data, deleted: st.deleted,
    rev: last ? `op:${last.op_id}` : base.rev, // §16.6: optimistische lokale Revision
    seq: base.seq, pending: mine.length > 0, base,
  };
}

export function createStubKernel(opts = {}) {
  const instanceId = opts.instanceId ?? 'stub';
  const userId = opts.userId ?? opts.actor?.id ?? 'local';
  const dbName = opts.dbName ?? `ctox-${instanceId}-${userId}`; // §16.7: DB je (Instanz, Nutzer)
  const latencyMs = opts.latencyMs ?? 50;
  const shellStartTimeoutMs = opts.shellStartTimeoutMs ?? 9000; // §16.19 Nr. 4: 9 s, Rückfall ≤ 10 s
  const generationGraceMs = opts.generationGraceMs ?? 10 * 60 * 1000; // §16.16: alte Generation 10 min halten
  const tabKey = opts.tabKey ?? sessionTabKey();
  const capabilities = new Set(opts.capabilities ?? []);
  const tabId = uuidv7();
  const channelName = `ctox-stub:${dbName}`;

  let db = null;
  let closed = false, loggedOut = false, loaded = false;
  let isLeader = false, myGen = 0, releaseLock = null, lockAbort = null;
  let bc = null, dropBroadcasts = false;
  const locks = opts.locks ?? globalThis.navigator?.locks;
  const hasLocks = !!locks?.request && typeof BroadcastChannel !== 'undefined' && opts.multiTab !== false;

  const mirror = new Map(); // ns -> Map(id -> record)
  let checkpoint = 0, localSeqKnown = 0;
  const activations = new Map();
  let link = { state: 'online', rtt_ms: latencyMs, relay: false };
  let extraBacklog = 0;
  const undelivered = [];
  let outboxStat = { count: 0, oldest_at: null };
  const waiters = new Map(); // op_id -> resolve(WriteResult)
  const cmdRejected = new Map();
  const zone = [];
  const routeListeners = new Map();
  const windowStates = new Map();
  let locale = opts.locale ?? 'de';
  const translations = {};
  let theme = { ...(opts.theme ?? { '--ctox-bg': '#ffffff', '--ctox-fg': '#1d1d1f', '--ctox-accent': '#2f6fed' }) };
  let actor = { id: userId, display_name: 'Lokaler Nutzer', role: 'admin', permissions: ['apps.source.view', 'apps.source.edit'], ...(opts.actor ?? {}) };
  const timers = new Set(), tasks = new Set();
  const inflight = new Map(); // docKey -> op_id
  const inflightOps = new Set();
  const skipBeforeCommit = new Set();
  const uploadProgress = new Map(), uploadsDone = new Set(), uploadResults = new Map();
  const activationListeners = new Set();
  const activationChains = new Map();
  let corruptNextFetch = 0;
  const publishing = new Set();
  const doubleWrite = new Map(); // ns_old -> {ns_new, migrate}
  const sim0 = { reject: [], rejectUpload: [], migrate: [], failCmd: new Map(), cmdHandlers: new Map(), cmdTypes: new Set() };
  const simRecord = { lastVisible: null, activationReports: [], navigations: [], dialogs: [], openedFiles: [], staleDiscards: 0, refetches: 0, connects: 1 };
  let readyResolve;
  const ready = new Promise((r) => { readyResolve = r; });
  const preReady = [];

  // ---- Zeitgeber und Aufgaben ----
  function later(fn, ms) {
    const t = setTimeout(() => { timers.delete(t); if (!closed) fn(); }, ms);
    timers.add(t);
    return t;
  }
  const sleep = (ms) => new Promise((r) => later(r, ms));
  function track(p) {
    tasks.add(p);
    p.catch((err) => { if (!closed) console.error('[ctox-stub]', err); }).finally(() => tasks.delete(p));
    return p;
  }
  function guardAlive() {
    if (closed || loggedOut) throw new CtoxError('unavailable', 'Kernel geschlossen (Abmeldung oder close)');
  }

  // ---- Live-Werte ----
  const tagIndex = new Map();
  const allLives = new Set();
  const dirty = new Set();
  let flushScheduled = false;
  const waitList = [];
  function notify(tag) {
    dirty.add(tag);
    if (!flushScheduled) { flushScheduled = true; queueMicrotask(flush); }
  }
  function flush() {
    flushScheduled = false;
    const hit = new Set();
    for (const t of dirty) for (const l of tagIndex.get(t) ?? []) hit.add(l);
    dirty.clear();
    for (const l of hit) l.refresh();
    for (const w of [...waitList]) {
      let ok = false;
      try { ok = w.pred(); } catch { ok = false; }
      if (ok) { waitList.splice(waitList.indexOf(w), 1); w.resolve(); }
    }
  }
  function waitUntil(pred) {
    if (pred()) return Promise.resolve();
    return new Promise((resolve) => waitList.push({ pred, resolve }));
  }
  function makeLive(tags, compute) {
    const subs = new Set();
    let last, has = false;
    const impl = {
      refresh() {
        if (!subs.size) return;
        const v = compute();
        if (has && liveEqual(last, v)) return;
        last = v; has = true;
        for (const cb of [...subs]) { try { cb(v); } catch (e) { console.error('[ctox-stub] subscriber', e); } }
      },
      dispose() { subs.clear(); for (const t of tags) tagIndex.get(t)?.delete(impl); },
    };
    return {
      get: () => compute(),
      subscribe(cb) {
        if (typeof cb !== 'function') throw new CtoxError('invalid', 'subscribe braucht eine Funktion');
        if (closed || loggedOut) return () => {};
        if (!subs.size) { for (const t of tags) { if (!tagIndex.has(t)) tagIndex.set(t, new Set()); tagIndex.get(t).add(impl); } allLives.add(impl); }
        subs.add(cb);
        last = compute(); has = true;
        try { cb(last); } catch (e) { console.error('[ctox-stub] subscriber', e); }
        return () => {
          if (!subs.delete(cb) || subs.size) return;
          for (const t of tags) tagIndex.get(t)?.delete(impl);
          allLives.delete(impl);
          has = false;
        };
      },
    };
  }

  // ---- Broadcast ----
  function post(msg) {
    if (!bc || closed) return;
    try { bc.postMessage({ ...msg, from: tabId }); } catch (e) { console.error('[ctox-stub] broadcast', e); }
  }

  // ---- Spiegel ----
  function mirrorGet(ns, id) { return mirror.get(ns)?.get(id) ?? null; }
  function visible(rec) { return rec && rec.data != null && !rec.deleted ? rec.data : null; }
  function mirrorPut(rec) {
    if (!rec) return false;
    let m = mirror.get(rec.collection);
    if (!m) { m = new Map(); mirror.set(rec.collection, m); }
    const prev = m.get(rec.id);
    if (prev && prev.rev === rec.rev && prev.seq === rec.seq && prev.pending === rec.pending && prev.deleted === rec.deleted && jsonEq(prev.data, rec.data)) return false;
    const r = { ...rec, data: rec.data == null ? null : deepFreeze(cloneValue(rec.data)) };
    m.set(rec.id, r);
    if (rec.lseq && rec.lseq > localSeqKnown) localSeqKnown = rec.lseq;
    notify(`c:${rec.collection}`);
    if (isLeader && (rec.collection === 'business_apps' || rec.collection === 'business_app_versions')) {
      const appId = rec.collection === 'business_apps' ? rec.id : visible(r)?.app_id;
      if (appId) scheduleActivation(appId);
    }
    if (rec.collection === 'business_commands') {
      if (rec.pending && rec.lseq) commandTimings.mark(rec.id, 'queued');
      if (!rec.pending && ['done','failed','cancelled','rejected'].includes(r.data?.status)) commandTimings.mark(rec.id, 'completed');
      notify('commands');
    }
    return true;
  }
  function setCheckpoint(cp) {
    if (cp > checkpoint) { checkpoint = cp; notify('checkpoint'); }
  }
  async function putDoc(t, rec) {
    const ls = ((await t.get('meta', 'local_seq'))?.value ?? 0) + 1;
    await t.put('meta', { key: 'local_seq', value: ls });
    rec.lseq = ls;
    await t.put('docs', rec);
  }
  async function bumpLocalSeq(t) {
    const ls = ((await t.get('meta', 'local_seq'))?.value ?? 0) + 1;
    await t.put('meta', { key: 'local_seq', value: ls });
    return ls;
  }
  async function assertGen(t) {
    const g = (await t.get('meta', 'leader_gen'))?.value ?? 0;
    if (!isLeader || g !== myGen) throw new StaleLeader('leader_gen veraltet');
  }
  async function getCore(t) { return (await t.get('sim_core', 'core')) ?? { key: 'core', head: 0 }; }

  // ---- Metriken ----
  const commandTimings = createCommandTimings((value, broadcast, patch) => {
    notify('metrics');
    if (broadcast) post({type:'command_timing', value:patch});
    try {
      const observed = opts.diagnostics?.onCommandTiming?.(value);
      if (observed != null) Promise.resolve(observed).catch(() => {});
    } catch { /* diagnostics cannot affect writes */ }
  });
  async function refreshOutboxStat() {
    if (!db || closed) return;
    const next = await db.outboxStat();
    if (next.count !== outboxStat.count || next.oldest_at !== outboxStat.oldest_at) { outboxStat = next; notify('metrics'); }
  }
  // §16.45 Collection-Liveness (gleiche Regeln wie Kernel #K-1e): je Collection im Spiegel bzw. abonniert
  // live = Verbindung oben und Backlog 0; catching_up = Backlog > 0 (noch nicht zugestellte Core-Änderungen dieser Collection
  // oder sim.setBacklog(n, collection)); offline = Verbindung weg; error = sim.failCollection(name, code).
  const collBacklog = new Map(), collError = new Map(), collSince = new Map();
  // §16.45: abonnierte Collections (aktivierte App-Manifeste, api.query/get) erscheinen SOFORT – catching_up bis zur ersten
  // vollständigen Zustellung nach dem Abonnieren, dann live, auch mit 0 Dokumenten. ns → complete
  const subscribed = new Map();
  function subscribeNs(ns) {
    if (subscribed.has(ns)) return;
    subscribed.set(ns, false);
    netChanged();
    const done = () => { if (!closed) { subscribed.set(ns, true); netChanged(); } };
    if (isLeader) track(Promise.resolve().then(() => deliver()).then(done, () => {}));
    else setTimeout(done, 0); // Leser: der Leiter stellt zu, der Spiegel kommt über BroadcastChannel
  }
  let collectionsView = {};
  function refreshCollections() {
    const names = new Set([...SYSTEM_COLLECTIONS, ...mirror.keys(), ...subscribed.keys(), ...collBacklog.keys(), ...collError.keys()]);
    const pending = new Map();
    for (const c of undelivered) pending.set(c.ns, (pending.get(c.ns) ?? 0) + 1);
    const view = {}, now = Date.now();
    for (const name of [...names].sort()) {
      const backlog = (pending.get(name) ?? 0) + (collBacklog.get(name) ?? 0);
      const state = collError.has(name) ? 'error' : link.state !== 'online' ? 'offline' : backlog > 0 || subscribed.get(name) === false ? 'catching_up' : 'live';
      const prev = collectionsView[name];
      const since = prev && prev.state === state ? prev.since_ms : (collSince.set(name, now), now);
      view[name] = { state, backlog, gen: genOf(name), since_ms: since, ...(collError.has(name) ? { error: { ...collError.get(name) } } : {}) };
    }
    collectionsView = view;
    return view;
  }
  function netChanged() {
    notify('metrics');
    if (isLeader) post({ type: 'net', link, backlog: undelivered.length + extraBacklog, collections: refreshCollections() });
  }
  let followerNet = null;
  const metricsLive = makeLive(['metrics'], () => {
    const net = isLeader || !followerNet ? { link, backlog: undelivered.length + extraBacklog, collections: refreshCollections() } : followerNet;
    return {
      commands: commandTimings.snapshot(),
      backlog: net.backlog,
      collections: cloneValue(net.collections ?? {}),
      outbox: { count: outboxStat.count, oldest_ms: outboxStat.oldest_at ? Math.max(0, Date.now() - outboxStat.oldest_at) : 0 },
      link: { ...net.link },
    };
  });
  Object.defineProperty(metricsLive, 'commands', {get: () => { guardAlive(); return deepFreeze(commandTimings.snapshot()); }});
  let knownGen = 0;
  const leaderLive = makeLive(['leader'], () => ({ isLeader, leader_gen: isLeader ? myGen : knownGen, tab_id: tabId }));

  // ---- Zustandsstrom (§16.19 Nr. 1): Kernel liefert Einträge, die Shell rendert sie (je Tab, live, je App filterbar) ----
  function zoneReport(appId, item, source) {
    if (!item || !['info', 'warn', 'error'].includes(item.level)) throw new CtoxError('invalid', 'level muss info|warn|error sein', { path: 'level' });
    if (typeof item.code !== 'string' || !item.code) throw new CtoxError('invalid', 'code fehlt', { path: 'code' });
    const entry = Object.freeze({ app_id: appId, level: item.level, code: item.code, text: String(item.text ?? ''), ref: item.ref ?? null, source, at: Date.now() });
    const i = zone.findIndex((e) => e.app_id === appId && e.code === item.code);
    if (i >= 0) zone[i] = entry; else zone.push(entry);
    notify('status');
  }
  function zoneClear(appId, code) {
    const i = zone.findIndex((e) => e.app_id === appId && e.code === code);
    if (i >= 0) { zone.splice(i, 1); notify('status'); }
  }
  const zoneLive = makeLive(['status'], () => zone.slice());
  const statusStream = Object.freeze({
    get: zoneLive.get,
    subscribe: zoneLive.subscribe,
    forApp: (appId) => makeLive(['status'], () => zone.filter((e) => e.app_id === appId)),
    /** Shell meldet eigene Einträge (z. B. Remount-Rückfall) in denselben Strom. */
    report: (appId, item) => zoneReport(appId, item, 'shell'),
    clear: (appId, code) => zoneClear(appId, code),
  });

  // ---- Laden, Leiterwahl ----
  async function loadFromStorage() {
    const docs = await db.all('docs');
    for (const r of docs) mirrorPut(r);
    const metas = await db.all('meta');
    for (const m of metas) {
      if (m.key.startsWith('upload_result:')) uploadResults.set(m.op_id,m);
      if (m.key === 'checkpoint') setCheckpoint(m.value);
      else if (m.key === 'local_seq') localSeqKnown = Math.max(localSeqKnown, m.value);
      else if (m.key === 'leader_gen') knownGen = m.value;
      else if (m.key.startsWith('activation:')) activations.set(m.app_id, m);
    }
    notify('activation');
    await refreshOutboxStat();
  }
  async function reconcile() {
    // §16.7: Leser gleichen meta.local_seq ab und holen verlorene Ereignisse nach.
    if (!db || closed || isLeader) return 0;
    const ls = (await db.get('meta', 'local_seq'))?.value ?? 0;
    if (ls <= localSeqKnown) return 0;
    const known = localSeqKnown;
    let n = 0;
    for (const r of await db.all('docs')) if ((r.lseq ?? 0) > known) { if (mirrorPut(r)) n++; }
    for (const m of await db.all('meta')) {
      if (m.key.startsWith('result:') && waiters.has(m.info.op_id)) handleWriteResult(m.info,m.result);
      if (m.key.startsWith('upload_result:')) { uploadResults.set(m.op_id,m);notify('uploads'); }
      if (m.key === 'checkpoint') setCheckpoint(m.value);
      if (m.key.startsWith('activation:')) applyActivationLocal(m, false);
    }
    localSeqKnown = Math.max(localSeqKnown, ls);
    await refreshOutboxStat();
    return n;
  }
  async function becomeLeader() {
    if (closed) return;
    myGen = await db.tx(async (t) => {
      const g = ((await t.get('meta', 'leader_gen'))?.value ?? 0) + 1;
      await t.put('meta', { key: 'leader_gen', value: g });
      return g;
    });
    isLeader = true;
    // Ein neuer Leiter hat alles gelesen, was der alte geschrieben hat (gemeinsame IDB).
    for (const r of await db.all('docs')) mirrorPut(r);
    for (const m of await db.all('meta')) {
      if (m.key.startsWith('activation:')) applyActivationLocal(m, false);
      // Takeover first advances the document mirror. Hydrate receipts here as
      // well; local_seq may already equal the checkpoint after that advance.
      if (m.key.startsWith('upload_result:')) { uploadResults.set(m.op_id,m);notify('uploads'); }
      if (m.key.startsWith('result:') && waiters.has(m.info.op_id)) handleWriteResult(m.info,m.result);
    }
    notify('leader');
    post({ type: 'leader', gen: myGen });
    netChanged();
    track(deliver());
    schedulePump();
    for (const id of mirror.get('business_apps')?.keys() ?? []) scheduleActivation(id);
  }
  function stepDown() {
    if (!isLeader) return;
    simRecord.staleDiscards++;
    isLeader = false;
    for (const op of inflightOps) inflightOps.delete(op);
    inflight.clear();
    notify('leader');
    const rel = releaseLock; releaseLock = null;
    if (rel) rel();
    if (closed) return;
    // neu anstellen; erneuter Erwerb erhöht leader_gen. Ohne Web Locks (Ein-Tab-Betrieb) sofort neu wählen.
    if (hasLocks) waitForLock();
    else track(becomeLeader());
  }
  function waitForLock() {
    if (!hasLocks) return;
    const ac = new AbortController();
    lockAbort = ac;
    locks.request(`ctox-stub-leader:${dbName}`, { signal: ac.signal }, async () => {
      if (closed) return;
      const hold = new Promise((r) => { releaseLock = r; });
      await becomeLeader();
      await hold;
    }).catch(() => {});
  }
  async function elect() {
    if (!hasLocks) { await becomeLeader(); return; }
    await new Promise((resolveInitial) => {
      locks.request(`ctox-stub-leader:${dbName}`, { ifAvailable: true }, async (lock) => {
        if (!lock) { resolveInitial(); waitForLock(); return; }
        const hold = new Promise((r) => { releaseLock = r; });
        await becomeLeader();
        resolveInitial();
        await hold;
      }).catch(() => resolveInitial());
    });
  }

  // ---- Broadcast-Empfang ----
  const rpcWaiters = new Map();
  async function onMessage(msg) {
    if (closed || dropBroadcasts || !msg || msg.from === tabId) return;
    switch (msg.type) {
      case 'docs': {
        for (const [ns, id] of msg.keys) mirrorPut(await db.get('docs', [ns, id]));
        if (msg.checkpoint) setCheckpoint(msg.checkpoint);
        if (msg.local_seq) localSeqKnown = Math.max(localSeqKnown, msg.local_seq);
        await refreshOutboxStat();
        break;
      }
      case 'command_timing': commandTimings.merge(msg.value); break;
      case 'outbox': await refreshOutboxStat(); if (isLeader) schedulePump(); break;
      case 'write_result': handleWriteResult(msg.info, msg.result); await refreshOutboxStat(); break;
      case 'net': followerNet = { link: msg.link, backlog: msg.backlog, collections: msg.collections ?? {} }; notify('metrics'); break;
      case 'hello': if (isLeader) netChanged(); break;
      case 'leader': knownGen = Math.max(knownGen, msg.gen ?? 0); notify('leader'); break;
      case 'activation': applyActivationLocal(msg.value, false); break;
      case 'drop_ns': dropNamespaces(msg.namespaces); break;
      case 'actor': actor = msg.actor; notify('actor'); break;
      case 'upload_result': uploadResults.set(msg.outcome.op_id,msg.outcome);notify('uploads');break;
      case 'upload': setUploadProgress(msg.ref, msg.sent, msg.total); break;
      case 'rpc': {
        if (!isLeader) break;
        try {
          const value = await simImpl[msg.name](...msg.args);
          post({ type: 'rpc_done', reqId: msg.reqId, value: value === undefined ? null : value });
        } catch (e) {
          post({ type: 'rpc_done', reqId: msg.reqId, error: { code: e?.code, message: String(e?.message ?? e) } });
        }
        break;
      }
      case 'rpc_done': {
        const w = rpcWaiters.get(msg.reqId);
        if (w) { rpcWaiters.delete(msg.reqId); msg.error ? w.rej(new CtoxError(msg.error.code ?? 'unavailable', msg.error.message)) : w.res(msg.value); }
        break;
      }
      default: break;
    }
  }
  // Core-Simulation läuft im Leiter; Leser leiten Sim-Aufrufe weiter (nur klonbare Argumente).
  function callLeader(name, args) {
    if (isLeader || !bc) {
      try { return Promise.resolve(simImpl[name](...args)); } catch (e) { return Promise.reject(e); } // Wirkung synchron, Fehler als Ablehnung
    }
    const reqId = uuidv7();
    return new Promise((res, rej) => {
      rpcWaiters.set(reqId, { res, rej });
      post({ type: 'rpc', reqId, name, args });
    });
  }

  // ---- Schreiben (§7.5, §16.6, §16.10) ----
  function handleFor(op_id, local, result, cancelFn) {
    local.catch(() => {});
    return { op_id, local, result, cancel: cancelFn };
  }
  function localWrite(app, op, collection, id, body, createOnly = false, options = {}) {
    const op_id = uuidv7();
    let resolveResult;
    const result = new Promise((r) => { resolveResult = r; });
    let entry = null;
    let reused = false;
    let ownCommandQueued = false;
    // Synchroner Teil: prüfen und optimistisch in den Spiegel (lokal sofort sichtbar, Flag pending).
    const prepare = () => {
      guardAlive();
      if (options.expected_revision !== undefined && (typeof options.expected_revision !== 'string' || !options.expected_revision)) throw new CtoxError('invalid','expected_revision must be a nonempty revision',{path:'expected_revision'});
      if (typeof collection !== 'string' || !collection) throw new CtoxError('invalid', 'collection fehlt', { path: 'collection' });
      if (typeof id !== 'string' || !id) throw new CtoxError('invalid', 'id (string) fehlt', { path: 'id' });
      if (collection === 'shell_preferences' && id !== actor.id) throw new CtoxError('forbidden', 'shell_preferences: nur die eigene Zeile (id = actor.id)', { path: 'id' });
      if (collection === 'app_drafts' && app.id !== 'ctox-platform') throw new CtoxError('forbidden', 'app_drafts nur über kernel.source(appId).draft.*', { path: 'collection' });
      const ns = resolveNs(app, collection);
      let clean = null;
      if (op === 'put') {
        clean = normalizeJson(body, '');
        if (!isPlainObject(clean)) throw new CtoxError('invalid', 'Dokument muss ein Objekt sein', { path: '' });
      } else if (op === 'patch') clean = normalizeJsonPatch(body);
      const cur = mirrorGet(ns, id);
      const st = applyOp(cur ? { data: cur.data, deleted: cur.deleted } : { data: null, deleted: false }, { op, body: clean, id });
      if (st.data) validateSchema(st.data, schemaFor(app, collection), ''); // lokale Schemaprüfung am Ergebnisdokument
      entry = {
        op_id, prev_op_id: cur?.pending && cur.rev?.startsWith('op:') ? cur.rev.slice(3) : null,
        collection: ns, id, op, body: clean, base_rev: cur?.rev ?? null, base_seq: cur?.base?.seq ?? 0,
        gen: genOf(ns), schema_ver: 1, app_id: app.id, app_ver: app.ver, created_at: Date.now(), origin_tab: tabId,
      };
      if (options.expected_revision !== undefined) entry.expected_revision = options.expected_revision;
      if (options.file_upload) { entry.file_upload = true; entry.file_upload_op = options.file_upload_op; }
      waiters.set(op_id, resolveResult);
      const base = cur?.base ?? emptyBase();
      if (!createOnly || !visible(cur)) mirrorPut({ collection: ns, id, data: st.data, deleted: st.deleted, rev: `op:${op_id}`, seq: base.seq, pending: true, base });
    };
    const persist = async () => {
      if (skipBeforeCommit.has(op_id)) {
        skipBeforeCommit.delete(op_id);
        waiters.delete(op_id);
        // optimistische Spiegelzeile zurücknehmen
        mirrorPut((await db.get('docs', [entry.collection, id])) ?? { collection: entry.collection, id, data: null, deleted: false, rev: null, seq: 0, pending: false, base: emptyBase() });
        return 'cancelled';
      }
      guardAlive();
      const ns = entry.collection;
      const rec = await db.tx(async (t) => {
        const stored = await t.get('docs', [ns, id]);
        // E26: the IDB transaction arbitrates concurrent tabs and reloads.
        // Reuse the original operation/result, never enqueue another command.
        if (createOnly && visible(stored)) {
          reused = true;
          waiters.delete(op_id);
          const pending = (await t.outboxFor(ns, id)).sort(byOpId).at(-1);
          if (pending) {
            const previous = waiters.get(pending.op_id);
            waiters.set(pending.op_id, (value) => { previous?.(value); resolveResult(value); });
          } else resolveResult({ status: 'accepted', rev: stored.rev, seq: stored.seq });
          return stored;
        }
        await t.put('outbox', entry); // docs + outbox in EINER Transaktion
        const r = recompute(ns, id, stored?.base ?? emptyBase(), await t.outboxFor(ns, id));
        await putDoc(t, r);
        return r;
      });
      ownCommandQueued = !reused;
      if (ns === 'business_commands' && rec.pending) commandTimings.mark(id, 'queued');
      mirrorPut(rec);
      post({ type: 'docs', keys: [[ns, id]], local_seq: rec.lseq });
      post({ type: 'outbox' });
      await refreshOutboxStat();
      if (isLeader) schedulePump();
      return 'ok';
    };
    let committed;
    try {
      if (loaded) { prepare(); committed = persist(); }
      else committed = new Promise((r) => preReady.push(r)).then(() => { prepare(); return persist(); });
    } catch (err) { committed = Promise.reject(err); }
    committed.then(
      (s) => { if (s === 'cancelled') resolveResult({ status: 'rejected', reason: 'cancelled' }); },
      (err) => {
        waiters.delete(op_id);
        resolveResult({ status: 'rejected', reason: err instanceof CtoxError ? err.code : 'local_error' });
      },
    );
    const localDone = committed.then(() => undefined);
    return handleFor(op_id, localDone, result, () => reused || (createOnly && !ownCommandQueued) ? false : cancelWrite(op_id, () => entry));
  }
  function normalizeJsonPatch(p) {
    if (!isPlainObject(p)) throw new CtoxError('invalid', 'patch muss ein Objekt sein', { path: '' });
    if (p.id !== undefined) throw new CtoxError('invalid', 'id ist im patch nicht änderbar', { path: 'id' });
    const keep = (v, path) => {
      if (v === null) return null; // Merge-Patch: null = Feld entfernen
      if (isPlainObject(v)) { const o = {}; for (const [k, x] of Object.entries(v)) if (x !== undefined) o[k] = keep(x, `${path}.${k}`); return o; }
      return normalizeJson(v, path);
    };
    const o = {};
    for (const [k, x] of Object.entries(p)) if (x !== undefined) o[k] = keep(x, k);
    return o;
  }
  function cancelWrite(op_id, getEntry) {
    if (closed || loggedOut) return false;
    const e = getEntry();
    if (!e) { skipBeforeCommit.add(op_id); return true; } // noch nicht vorbereitet
    // nur vor dem Senden: Leiter weiß es exakt; ein Leser nur, wenn der Leiter offline ist.
    const canCancel = isLeader ? !inflightOps.has(op_id) : (followerNet?.link?.state ?? 'online') !== 'online';
    if (!canCancel) return false;
    track((async () => {
      const rec = await db.tx(async (t) => {
        if (!(await t.get('outbox', op_id))) return null;
        await t.del('outbox', op_id);
        const stored = await t.get('docs', [e.collection, e.id]);
        const r = recompute(e.collection, e.id, stored?.base ?? emptyBase(), await t.outboxFor(e.collection, e.id));
        await putDoc(t, r);
        return r;
      });
      if (!rec) return;
      mirrorPut(rec);
      post({ type: 'docs', keys: [[e.collection, e.id]], local_seq: rec.lseq });
      const res = waiters.get(op_id);
      waiters.delete(op_id);
      res?.({ status: 'rejected', reason: 'cancelled' });
      await refreshOutboxStat();
      post({ type: 'outbox' });
    })());
    return true;
  }
  function handleWriteResult(info, result) {
    if (info.collection === 'business_commands') {
      commandTimings.mark(info.id, 'acked');
      if (result.status === 'rejected') commandTimings.mark(info.id, 'completed');
    }
    const res = waiters.get(info.op_id);
    if (res) { waiters.delete(info.op_id); res(cloneValue(result)); }
    if (result.status === 'rejected' && result.reason !== 'cancelled') {
      zoneReport(info.app_id, { level: 'error', code: `write.rejected:${info.op_id}`, text: `Änderung abgelehnt: ${result.reason}`, ref: result.archived_ref }, 'kernel');
      if (info.collection === 'business_commands') { cmdRejected.set(info.id, result.reason); notify('commands'); }
    }
  }

  // ---- Leiter: Outbox senden, simulierter Core bestätigt ----
  let pumping = false, pumpAgain = false, pumpScheduled = false;
  function schedulePump() {
    if (pumpScheduled || closed) return;
    pumpScheduled = true;
    later(() => { pumpScheduled = false; track(pump()); }, 0);
  }
  async function pump() {
    if (!isLeader || closed || link.state !== 'online') return;
    if (pumping) { pumpAgain = true; return; }
    pumping = true;
    try {
      do {
        pumpAgain = false;
        const entries = (await db.all('outbox')).sort(byOpId);
        const first = new Map();
        for (const e of entries) { const k = docKey(e.collection, e.id); if (!first.has(k)) first.set(k, e); }
        for (const [k, e] of first) {
          if (inflight.has(k) || !isLeader || link.state !== 'online') continue;
          inflight.set(k, e.op_id);
          inflightOps.add(e.op_id);
          if (e.op === 'upload') setUploadProgress(e.id, Math.floor((uploadProgress.get(e.id)?.total ?? 0) / 2), uploadProgress.get(e.id)?.total ?? 0, true);
          later(() => track(confirm(k, e)), latencyMs);
        }
      } while (pumpAgain);
    } finally { pumping = false; }
  }
  function takeOutcome() {
    if (sim0.reject.length) { const rejection=sim0.reject.shift(); return {status:'rejected',...(typeof rejection === 'object' ? rejection : {reason:rejection})}; }
    if (sim0.migrate.length) return { status: 'migrated', dropFields: sim0.migrate.shift().dropFields ?? [] };
    return null;
  }
  // Durable CAS outcomes are per upload operation, not just per content hash.
  // This separates a new upload from an earlier accepted/rejected same-ref attempt.
  async function storeUploadResult(t, entry, result) {
    const outcome={key:`upload_result:${entry.op_id}`,op_id:entry.op_id,ref:entry.id,...result};
    await t.put('meta',outcome);
    let rec=null;
    if (result.status === 'rejected' && entry.file_id) {
      const row=await t.get('docs',['files',entry.file_id]);
      if (row?.data?.ref === entry.id && row.data.owner_app === entry.app_id) {
        rec={...row,data:{...row.data,state:'rejected'},deleted:false,local_file_rejection:true};
        await putDoc(t,rec);
      }
    }
    return {outcome,rec};
  }
  function publishUploadResult(value) {
    uploadResults.set(value.outcome.op_id,value.outcome);notify('uploads');
    if(value.rec){mirrorPut(value.rec);post({type:'docs',keys:[['files',value.rec.id]]});}
    post({type:'upload_result',outcome:value.outcome});
  }
  async function confirm(k, e) {
    const done = () => { if (inflight.get(k) === e.op_id) inflight.delete(k); inflightOps.delete(e.op_id); };
    if (closed || !isLeader || link.state !== 'online') { done(); return; }
    const outcome = e.op === 'upload' ? null : takeOutcome();
    let out;
    try {
      out = await db.tx(async (t) => {
        await assertGen(t);
        if (!(await t.get('outbox', e.op_id))) return null;
        if (e.op === 'upload') {
          const b = await t.get('blobs', e.id);
          const rejected=sim0.rejectUpload.shift();
          if (rejected) {
            await t.del('outbox',e.op_id);
            const uploadResult=await storeUploadResult(t,e,{status:'rejected',...(typeof rejected === 'object' ? rejected : {code:'forbidden',reason:rejected})});
            await bumpLocalSeq(t);return {upload:true,rejected:true,total:b?.size ?? 0,uploadResult};
          }
          if (b) {
            await t.put('sim_core', { key: `blob:${e.id}`, blob: b.blob, type: b.type, size: b.size });
            await t.put('blobs', { ...b, pinned: false });
          }
          await t.del('outbox', e.op_id);
          const uploadResult=await storeUploadResult(t,e,{status:'accepted'});
          await bumpLocalSeq(t);
          return { upload: true, total: b?.size ?? 0, uploadResult };
        }
        const core = await getCore(t);
        const stored = await t.get('docs', [e.collection, e.id]);
        const base = stored?.base ?? emptyBase();
        let res = (await t.get('sim_core', `op:${e.op_id}`))?.result ?? null;
        let newBase = base;
        if (!res) {
          res = outcome;
          if (!res && e.expected_revision !== undefined && e.expected_revision !== base.rev) res = {status:'rejected',code:'revision_conflict',reason:'Expected revision differs from current revision'};
          if (!res && e.op === 'patch' && (base.data == null || base.deleted)) res = { status: 'rejected', reason: 'not_found' };
          if (!res && e.op === 'delete') {
            let br = e.base_rev;
            if (br && br.startsWith('op:')) br = (await t.get('sim_core', `op:${br.slice(3)}`))?.result?.rev ?? br;
            if ((br ?? null) !== (base.rev ?? null)) res = { status: 'rejected', reason: 'conflict' };
          }
          if(!res && e.collection==='business_commands' && e.op==='put' && e.body?.status==='queued') {
            const route=commandRoute(e.body,routingManifest,actor,sim0.cmdHandlers.has(e.body.type)||sim0.cmdTypes.has(e.body.type)||sim0.failCmd.has(e.body.type));
            if(route.error)res={status:'rejected',code:route.error,reason:route.error};
          }
          // §16.44: ctox.coding.* prüft der Core beim Write (exakte Codes, Reservierung des Entwurfs im selben Schritt)
          if (!res && e.collection === 'business_commands' && e.op === 'put' && e.body?.status === 'queued' && String(e.body.type).startsWith('ctox.coding.')) {
            res = await checkCodingCommand(t, e.body);
          }
          // K-WJ-B2 (E27.2): Befehlstyp ohne veröffentlichte Route → beim Schreiben abgelehnt, nie erst angenommen
          if (!res && e.collection === 'business_commands' && e.op === 'put' && e.body?.status === 'queued' && !commandRouteKnown(e.body.app_id, String(e.body.type))) {
            res = { status: 'rejected', code: 'unknown_command', reason: `unknown_command:${e.body.type}` };
          }
          if (!res) res = { status: 'accepted' };
          if (res.status !== 'rejected') {
            core.head += 1;
            const seq = core.head;
            let st = applyOp(base, e);
            let archived_ref;
            if (res.status === 'migrated' && st.data) {
              const d = { ...st.data }, dropped = {};
              for (const f of res.dropFields) if (f in d && f !== 'id') { dropped[f] = d[f]; delete d[f]; }
              st = { ...st, data: d };
              archived_ref = `core_archive:${e.op_id}`;
              await t.put('sim_core', { key: `archive:${e.op_id}`, ref: archived_ref, collection: e.collection, id: e.id, fields: dropped, at: Date.now() });
            }
            const rev = `${seq}-${fnv1a(JSON.stringify(st.data))}`;
            res = { status: res.status, rev, seq, ...(archived_ref ? { archived_ref } : {}) };
            newBase = { data: st.data, deleted: st.deleted, rev, seq };
            await t.put('sim_core', core);
          } else res = { status: 'rejected', reason: res.reason, ...(res.code ? { code: res.code } : {}) };
          await t.put('sim_core', { key: `op:${e.op_id}`, result: res });
        }
        if (res.status === 'rejected') {
          res = { ...res, archived_ref: `archive_local:${e.op_id}` };
          await t.put('archive_local', {
            ref: res.archived_ref, op_id: e.op_id, collection: e.collection, id: e.id, op: e.op, body: e.body,
            reason: res.reason, app_id: e.app_id, app_ver: e.app_ver, created_at: e.created_at, rejected_at: Date.now(),
          });
        }
        await t.put('meta',{key:`result:${e.op_id}`,info:{op_id:e.op_id,app_id:e.app_id,collection:e.collection,id:e.id},result:res});
        await t.del('outbox', e.op_id);
        const rec = recompute(e.collection, e.id, newBase, await t.outboxFor(e.collection, e.id));
        if (e.file_upload && (res.status === 'rejected' || (await t.get('meta',`upload_result:${e.file_upload_op}`))?.status === 'rejected') && !rec.pending) { rec.data={...e.body,state:'rejected'};rec.deleted=false;rec.local_file_rejection=true; }
        await putDoc(t, rec);
        let cp = (await t.get('meta', 'checkpoint'))?.value ?? 0;
        if (newBase.seq > cp) { cp = newBase.seq; await t.put('meta', { key: 'checkpoint', value: cp }); }
        return { res, rec, cp };
      });
    } catch (err) {
      done();
      if (err instanceof StaleLeader) { stepDown(); return; }
      throw err;
    }
    done();
    if (!out) { schedulePump(); return; }
    if (e.collection === 'business_commands') {

      commandTimings.mark(e.id, 'sent');
    }
    if (out.upload) {
      publishUploadResult(out.uploadResult);
      if (!out.rejected) {
      setUploadProgress(e.id, out.total, out.total, true);
      uploadsDone.add(e.id);
      notify('uploads');
      post({ type: 'upload', ref: e.id, sent: out.total, total: out.total, done: true });
      }
    } else {
      mirrorPut(out.rec);
      setCheckpoint(out.cp);
      post({ type: 'docs', keys: [[e.collection, e.id]], checkpoint: out.cp, local_seq: out.rec.lseq });
      const info = { op_id: e.op_id, app_id: e.app_id, collection: e.collection, id: e.id };
      handleWriteResult(info, out.res);
      post({ type: 'write_result', info, result: out.res });
      if (out.res.status !== 'rejected') {
        if (e.collection === 'business_commands' && e.op === 'put' && e.body?.status === 'queued') track(runCommand(e.body));
        const dw = doubleWrite.get(e.collection);
        if (dw) {
          const d = out.rec.base.data;
          track(corePush(dw.ns_new, [d && !out.rec.base.deleted ? { ...dw.migrate(cloneValue(d)), id: e.id } : { id: e.id, _deleted: true }]));
        }
      }
    }
    await refreshOutboxStat();
    schedulePump();
  }

  // ---- simulierter Core: Push, Zustellung ----
  async function corePush(ns, docs) {
    const list = Array.isArray(docs) ? docs : [docs];
    const changes = await db.tx(async (t) => {
      const core = await getCore(t);
      const outList = [];
      for (const d of list) {
        if (!d || typeof d.id !== 'string' || !d.id) throw new CtoxError('invalid', 'push: Dokument ohne id', { path: 'id' });
        core.head += 1;
        const deleted = !!d._deleted;
        let data = null;
        if (!deleted) { data = normalizeJson({ ...d }, ''); delete data._deleted; }
        outList.push({ ns, id: d.id, data, deleted, seq: core.head, rev: `${core.head}-${fnv1a(JSON.stringify(data))}` });
      }
      await t.put('sim_core', core);
      return outList;
    });
    undelivered.push(...changes);
    netChanged();
    if (link.state === 'online') await deliver();
    return changes.map((c) => c.seq);
  }
  async function coreView(ns, id) {
    for (let i = undelivered.length - 1; i >= 0; i--) if (undelivered[i].ns === ns && undelivered[i].id === id) return undelivered[i].data;
    const stored = await db.get('docs', [ns, id]);
    return stored?.base?.data ?? null;
  }
  async function coreMerge(ns, id, patch) {
    const cur = (await coreView(ns, id)) ?? { id };
    const next = mergePatch(cur, patch);
    // Befehlsergebnis ist ein Wert, kein Merge-Patch: null-Felder (z. B. exit_code bei Zeitüberschreitung) bleiben erhalten
    if (ns === 'business_commands' && isPlainObject(patch) && Object.prototype.hasOwnProperty.call(patch, 'result')) next.result = cloneValue(patch.result);
    next.id = id;
    return corePush(ns, [next]);
  }
  let delivering = false;
  async function deliver() {
    if (delivering || !isLeader) return;
    delivering = true;
    try {
      while (undelivered.length && link.state === 'online' && !closed && isLeader) {
        const batch = undelivered.splice(0);
        let out;
        try {
          out = await db.tx(async (t) => {
            await assertGen(t);
            let cp = (await t.get('meta', 'checkpoint'))?.value ?? 0;
            const recs = [];
            for (const c of batch) {
              const stored = await t.get('docs', [c.ns, c.id]);
              if (!stored || (stored.base?.seq ?? 0) < c.seq) { // §16.1: nur anwenden, wenn seq größer
                const r = recompute(c.ns, c.id, { data: c.data, deleted: c.deleted, rev: c.rev, seq: c.seq }, await t.outboxFor(c.ns, c.id));
                await putDoc(t, r);
                recs.push(r);
              }
              if (c.seq > cp) cp = c.seq;
            }
            await t.put('meta', { key: 'checkpoint', value: cp }); // Dokumente und Checkpoint in einer Transaktion
            return { recs, cp };
          });
        } catch (err) {
          undelivered.unshift(...batch);
          if (err instanceof StaleLeader) { stepDown(); return; }
          throw err;
        }
        for (const r of out.recs) mirrorPut(r);
        setCheckpoint(out.cp);
        post({ type: 'docs', keys: out.recs.map((r) => [r.collection, r.id]), checkpoint: out.cp, local_seq: out.recs.at(-1)?.lseq });
        netChanged();
      }
    } finally { delivering = false; }
  }

  // ---- K-1i: process.run (CAP-3d #844, §16.47) – Allowlist der Plattform, Grund-Umgebung, stdin/stdin_ref; SIMULIERT ----
  // Kein echter Prozess: die Ausführung ist deterministisch nachgebildet (cat → stdin, echo → args, env → Umgebung,
  // false → exit 1, sleep N → timed_out, wenn N·1000 > timeout_ms; sonst JSON-Beschreibung des Aufrufs).
  const DEFAULT_BASE_ENV = Object.freeze({ PATH: '/usr/local/bin:/usr/bin:/bin', CTOX_BIN: '/opt/ctox/bin', LANG: 'C.UTF-8' });
  const DEFAULT_PROCESS_ALLOWLIST = Object.freeze({
    cat: { program: '/bin/cat', env: DEFAULT_BASE_ENV },
    echo: { program: '/bin/echo', env: DEFAULT_BASE_ENV },
    env: { program: '/usr/bin/env', env: DEFAULT_BASE_ENV },
    false: { program: '/usr/bin/false', env: DEFAULT_BASE_ENV },
    sleep: { program: '/bin/sleep', env: DEFAULT_BASE_ENV },
    scrape: { interpreter: '/usr/local/bin/tsx', script: '/opt/ctox/scripts/scrape.ts', env: DEFAULT_BASE_ENV },
  });
  const STDIN_MAX = 1024 * 1024, ENV_VALUE_MAX = 4096;
  const PROCESS_FIELDS = ['program', 'args', 'env', 'stdin', 'stdin_ref', 'timeout_ms'];
  async function simulateProcessRun(appId, req, allowlist, readOwnFile) {
    const bad = (code, reason) => { throw new CtoxError(code, reason); };
    if (!isPlainObject(req)) bad('invalid', 'payload_invalid:request');
    for (const k of Object.keys(req)) if (!PROCESS_FIELDS.includes(k)) bad('invalid', `payload_invalid:${k}`);
    if (typeof req.program !== 'string' || !req.program) bad('invalid', 'payload_invalid:program');
    const raw = (allowlist ?? DEFAULT_PROCESS_ALLOWLIST)[req.program];
    if (!raw) bad('forbidden', 'program_not_allowed');
    const entry = typeof raw === 'string' ? { program: raw, env: {} } : raw;
    const args = req.args ?? [];
    if (!Array.isArray(args) || args.some((a) => typeof a !== 'string')) bad('invalid', 'payload_invalid:args');
    const baseEnv = { ...(entry.env ?? {}) };
    const appEnv = req.env ?? {};
    if (!isPlainObject(appEnv)) bad('invalid', 'payload_invalid:env');
    for (const [key, v] of Object.entries(appEnv)) {
      if (typeof v !== 'string' || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) bad('invalid', 'payload_invalid:env');
      if (Object.prototype.hasOwnProperty.call(baseEnv, key)) bad('invalid', `env_base_override:${key}`); // Grundvariable nie ersetzbar
      if (new TextEncoder().encode(v).byteLength > ENV_VALUE_MAX) bad('invalid', `env_value_too_large:${key}`);
    }
    if (req.stdin !== undefined && req.stdin_ref !== undefined) bad('invalid', 'payload_invalid:stdin');
    let stdin = '';
    if (req.stdin !== undefined) {
      if (typeof req.stdin !== 'string') bad('invalid', 'payload_invalid:stdin');
      if (new TextEncoder().encode(req.stdin).byteLength > STDIN_MAX) bad('invalid', 'stdin_too_large');
      stdin = req.stdin;
    } else if (req.stdin_ref !== undefined) {
      if (!isPlainObject(req.stdin_ref) || typeof req.stdin_ref.ref !== 'string') bad('invalid', 'payload_invalid:stdin_ref');
      const f = await readOwnFile(req.stdin_ref.ref); // nur files-Datensätze der aufrufenden App (sonst forbidden)
      if (!f?.found) bad('not_found', 'stdin_ref_not_found');
      if (typeof f.text !== 'string') bad('invalid', 'stdin_ref_not_utf8');
      if (new TextEncoder().encode(f.text).byteLength > STDIN_MAX) bad('invalid', 'stdin_too_large');
      stdin = f.text;
    }
    const timeout = req.timeout_ms ?? 30000;
    if (!Number.isInteger(timeout) || timeout <= 0) bad('invalid', 'payload_invalid:timeout_ms');
    const env = { ...baseEnv, ...appEnv }; // Kind = leere Umgebung + Grund-Umgebung + App-env
    const argv = entry.interpreter ? [entry.interpreter, entry.script, ...args] : [entry.program, ...args];
    const name = String(argv[entry.interpreter ? 1 : 0]).split('/').pop();
    let exit_code = 0, stdout = '', stderr = '', timed_out = false;
    if (name === 'cat') stdout = stdin;
    else if (name === 'echo') stdout = `${args.join(' ')}\n`;
    else if (name === 'env') stdout = Object.keys(env).sort().map((k) => `${k}=${env[k]}`).join('\n') + '\n';
    else if (name === 'false') exit_code = 1;
    else if (name === 'sleep') { if (Number(args[0] ?? 0) * 1000 > timeout) { timed_out = true; exit_code = null; stderr = `timeout after ${timeout} ms\n`; } }
    else stdout = `${JSON.stringify({ app: appId, argv, env_keys: Object.keys(env).sort(), stdin_bytes: new TextEncoder().encode(stdin).byteLength })}\n`;
    return { exit_code, stdout, stderr, timed_out };
  }

  // stdin_ref: nur files-Datensätze der aufrufenden App (sonst forbidden)
  async function readOwnFileForProcess(appId, ref) {
    const docs = [...(mirror.get('files')?.values() ?? [])].map((r) => visible(r)).filter((d) => d && d.ref === ref);
    if (!docs.length) return { found: false };
    if (!docs.some((d) => d.owner_app === appId)) throw new CtoxError('forbidden', 'stdin_ref gehört einer anderen App');
    const b = (await db.get('blobs', ref))?.blob ?? (await db.get('sim_core', `blob:${ref}`))?.blob;
    if (!b) return { found: false };
    const bytes = new Uint8Array(await b.arrayBuffer());
    try { return { found: true, text: new TextDecoder('utf-8', { fatal: true }).decode(bytes) }; } catch { return { found: true }; }
  }
  function capProcessRun(appId) {
    return async (req) => {
      const declared = new Set(visible(mirrorGet('business_app_versions', `${appId}@${visible(mirrorGet('business_apps', appId))?.active_ver}`))?.manifest?.capabilities_required ?? []);
      if (!declared.has('process') && !declared.has('process.run')) throw new CtoxError('forbidden', `cap.process.run nicht in capabilities_required von ${appId}`);
      return simulateProcessRun(appId, req, processAllowlist, (ref) => readOwnFileForProcess(appId, ref));
    };
  }
  let processAllowlist = null;
  // ---- K-1g: simulierte Modellantwort (Responses-förmig, deterministisch; gleiche Regeln wie Kernel-Fake-Core) ----
  function simulatedModelRun(appId, req) {
    if (!isPlainObject(req)) throw new CtoxError('invalid', 'model.run: Anfrage-Objekt nötig');
    const input = req.input ?? req.prompt ?? req.messages;
    let text;
    if (typeof input === 'string') text = input;
    else if (Array.isArray(input)) text = input.map((m) => (typeof m === 'string' ? m : typeof m?.content === 'string' ? m.content : Array.isArray(m?.content) ? m.content.map((c) => c?.text ?? '').join(' ') : '')).join('\n');
    else throw new CtoxError('invalid', 'model.run: input, prompt oder messages nötig');
    const model = typeof req.model === 'string' && req.model ? req.model : 'fake-core';
    const out = `Simulierte Modellantwort (${model}) für ${appId}: ${text.replace(/\s+/g, ' ').trim().slice(0, 200)}`;
    return {
      id: `resp_${fnv1a(JSON.stringify([appId, model, text]))}`, object: 'response', model, status: 'completed',
      output: [{ type: 'message', role: 'assistant', content: [{ type: 'output_text', text: out }] }],
      output_text: out, usage: { input_tokens: text.split(/\s+/).filter(Boolean).length, output_tokens: out.split(/\s+/).length },
    };
  }
  // ---- §16.44 Coding Agent im Quelltext-Modus (simuliert; Vertrag befund-coding-panel.md / Core-PR #848) ----
  // Wie Kernel-Fake-Core K-1f: der simulierte Agent hängt an die erste im Prompt genannte Textdatei des Entwurfs (sonst die
  // erste) eine Kommentarzeile an; Rückschreiben per CAS auf base_draft_rev; Nachrichten/tool_events erst am Turn-Ende.
  const TURN_FIELDS = ['app_id', 'draft_id', 'base_draft_rev', 'prompt', 'model', 'turn_id'];
  const activeTurns = new Map(); // draft_id → turn_id
  const turnByCommand = new Map(); // command_id → turn_id
  const coreDoc = async (t, ns, id) => { const r = await t.get('docs', [ns, id]); return r?.base?.data && !r.base.deleted ? { data: r.base.data, rev: r.base.rev } : null; };
  async function checkCodingCommand(t, body) {
    const p = isPlainObject(body.payload) ? body.payload : {};
    const inv = (f) => ({ status: 'rejected', code: 'invalid', reason: `payload_invalid:${f}` });
    const rej = (code, reason) => ({ status: 'rejected', code, reason });
    if (body.type === 'ctox.coding.turn') {
      for (const k of Object.keys(p)) if (!TURN_FIELDS.includes(k)) return inv(k);
      for (const f of ['app_id', 'draft_id', 'base_draft_rev', 'prompt']) if (typeof p[f] !== 'string' || !p[f].trim()) return inv(f);
      if (p.model !== undefined && (typeof p.model !== 'string' || !p.model)) return inv('model');
      if (p.turn_id !== undefined && (typeof p.turn_id !== 'string' || !/^[A-Za-z0-9_.:-]{1,128}$/.test(p.turn_id))) return inv('turn_id');
      if (!actor.permissions.includes('apps.source.edit')) return rej('forbidden', 'apps.source.edit');
      const d = await coreDoc(t, 'app_drafts', p.draft_id);
      if (!d) return rej('not_found', 'draft_not_found');
      if (d.data.user_id !== body.actor_id || d.data.app_id !== p.app_id) return rej('forbidden', 'draft_not_owned'); // auch Admin nicht
      if (d.rev !== p.base_draft_rev) return rej('conflict', 'stale_draft_rev');
      if (activeTurns.has(p.draft_id)) return rej('conflict', 'turn_running');
      const taken = p.turn_id ? await coreDoc(t, 'coding_turns', p.turn_id) : null;
      if (taken && taken.data.command_id !== body.id) return rej('conflict', 'turn_id_taken');
      const turn_id = p.turn_id ?? `turn-${uuidv7()}`;
      activeTurns.set(p.draft_id, turn_id); // Prüfung + Reservierung in einem Schritt
      turnByCommand.set(body.id, turn_id);
      return null;
    }
    if (body.type === 'ctox.coding.cancel') {
      for (const k of Object.keys(p)) if (k !== 'turn_id') return inv(k);
      if (typeof p.turn_id !== 'string' || !p.turn_id) return inv('turn_id');
      const turn = await coreDoc(t, 'coding_turns', p.turn_id);
      if (!turn) return rej('not_found', 'turn_not_found');
      if (turn.data.user_id !== body.actor_id) return rej('forbidden', 'turn_not_owned');
      if (!['queued', 'running'].includes(turn.data.state)) return rej('conflict', 'turn_not_running');
      return null;
    }
    return inv('type');
  }
  const turnNow = async (id) => (await coreView('coding_turns', id));
  async function setTurn(id, patch) {
    const cur = await turnNow(id);
    if (!cur) return null;
    if (patch.state && !['queued', 'running'].includes(cur.state)) return cur; // Endzustand genau einmal
    const next = { ...cur, ...patch, updated_at_ms: Date.now() };
    await corePush('coding_turns', [next]);
    return next;
  }
  // Unified diff mit 3 Kontextzeilen je Hunk (gleiche Funktion wie Kernel-Fake-Core K-1h)
  function unifiedDiff(path, before, after, context = 3) {
    const split = (t) => (t === '' ? [] : t.replace(/\n$/, '').split('\n'));
    const a = split(before), b = split(after);
    let pre = 0;
    while (pre < a.length && pre < b.length && a[pre] === b[pre]) pre++;
    let suf = 0;
    while (suf < a.length - pre && suf < b.length - pre && a[a.length - 1 - suf] === b[b.length - 1 - suf]) suf++;
    const from = Math.max(0, pre - context);
    const aEnd = Math.min(a.length, a.length - suf + context), bEnd = Math.min(b.length, b.length - suf + context);
    const lines = [];
    for (let i = from; i < pre; i++) lines.push(` ${a[i]}`);
    for (let i = pre; i < a.length - suf; i++) lines.push(`-${a[i]}`);
    for (let i = pre; i < b.length - suf; i++) lines.push(`+${b[i]}`);
    for (let i = a.length - suf; i < aEnd; i++) lines.push(` ${a[i]}`);
    const aLen = aEnd - from, bLen = bEnd - from;
    return `--- a/${path}\n+++ b/${path}\n@@ -${aLen ? from + 1 : from},${aLen} +${bLen ? from + 1 : from},${bLen} @@\n${lines.join('\n')}\n`;
  }
  async function runCodingTurn(doc) {
    const p = doc.payload, turn_id = turnByCommand.get(doc.id) ?? p.turn_id;
    const now = Date.now();
    await corePush('coding_turns', [{ id: turn_id, app_id: p.app_id, draft_id: p.draft_id, user_id: doc.actor_id, command_id: doc.id, origin_app: doc.app_id,
      base_draft_rev: p.base_draft_rev, prompt: p.prompt, model: p.model ?? null, state: 'queued', messages: [], tool_events: [], draft_rev_after: null, changed_files: [],
      callback: { on_complete: doc.on_complete ?? null, on_failed: doc.on_failed ?? null }, created_at_ms: now, updated_at_ms: now }]);
    const finish = async (state, extra) => {
      activeTurns.delete(p.draft_id);
      const t = (await setTurn(turn_id, { state, finished_at_ms: Date.now(), ...extra })) ?? (await turnNow(turn_id));
      const result = { turn_id, state: t.state, draft_rev_after: t.draft_rev_after ?? null, changed_files: t.changed_files ?? [], ...(t.patch ? { patch: t.patch } : {}), ...(t.error ? { error: t.error } : {}) };
      const type = t.state === 'done' ? t.callback?.on_complete : t.callback?.on_failed;
      const cbId = `coding-callback:${doc.id}`;
      if (type && !(await coreView('business_commands', cbId))) { // höchstens einmal
        const cb = { id: cbId, type, payload: { command_id: doc.id, turn_id, status: t.state, result }, status: 'queued', app_id: doc.app_id, actor_id: 'core', created_at: new Date().toISOString() };
        await corePush('business_commands', [cb]);
        track(runCommand(cb));
      }
      if (t.state === 'done') return { status: 'done', progress: 1, result };
      if (t.state === 'cancelled') return { status: 'cancelled', result };
      return { status: 'failed', error: `${t.error?.code ?? 'failed'}: ${t.error?.reason ?? t.state}`, result };
    };
    await setTurn(turn_id, { state: 'running' });
    // §16.44-Ergänzung (1): Arbeitsbaum = VOLLER Baum = Basisversion + Entwurfs-Überlagerung (null = gelöscht)
    const start = await coreView('app_drafts', p.draft_id);
    const files = {};
    const baseFiles = start?.base_app_ver ? versionDoc(p.app_id, start.base_app_ver)?.manifest?.files ?? {} : {};
    for (const [path, hash] of Object.entries(baseFiles)) {
      const c = (await db.get('sim_core', `file:${hash}`))?.content;
      if (typeof c === 'string') files[path] = c;
    }
    for (const [path, v] of Object.entries(start?.files ?? {})) { if (v === null) delete files[path]; else files[path] = v; }
    await sleep(opts.codingDelayMs ?? 60);
    if ((await turnNow(turn_id)).state === 'cancelled') return finish('cancelled', {});
    const textPaths = Object.keys(files).filter((x) => typeof files[x] === 'string').sort();
    const path = textPaths.find((x) => p.prompt.includes(x)) ?? textPaths[0] ?? 'NOTES.md';
    const old = typeof files[path] === 'string' ? files[path] : '';
    const line = `// coding-agent ${turn_id}: ${p.prompt.replace(/\s+/g, ' ').slice(0, 60)}`;
    const next = `${old}${old && !old.endsWith('\n') ? '\n' : ''}${line}\n`;
    const at = Date.now();
    const transcript = {
      messages: [{ role: 'user', text: p.prompt, at_ms: at }, { role: 'assistant', text: `Kommentarzeile in ${path} ergänzt.`, at_ms: at }],
      tool_events: [{ kind: 'read', path, at_ms: at }, { kind: 'edit', path, at_ms: at }],
    };
    if ((await turnNow(turn_id)).state === 'cancelled') return finish('cancelled', transcript);
    const curRow = await db.get('docs', ['app_drafts', p.draft_id]);
    const cur = curRow?.base;
    if (cur?.data && !cur.deleted && cur.rev === p.base_draft_rev) { // CAS auf base_draft_rev
      await corePush('app_drafts', [{ ...cur.data, files: { ...cur.data.files, [path]: next }, updated_at: at }]);
      const rev = (await db.get('docs', ['app_drafts', p.draft_id]))?.base?.rev ?? null;
      return finish('done', { ...transcript, draft_rev_after: rev, changed_files: [path] });
    }
    // Nutzer hat inzwischen editiert: nichts überschreiben, Patch als files-Datensatz coding-patch:<turn_id>
    const patch = unifiedDiff(path, old, next); // §16.44-Ergänzung (4): 3 Kontextzeilen, Basis = base_draft_rev
    const blob = new Blob([patch], { type: 'text/x-diff' });
    const ref = await sha256Hex(blob);
    await db.tx((t) => t.put('sim_core', { key: `blob:${ref}`, blob, type: blob.type, size: blob.size }));
    const file_id = `coding-patch:${turn_id}`;
    await corePush('files', [{ id: file_id, ref, name: `${file_id.replace(':', '-')}.diff`, mime: 'text/x-diff', size: blob.size, owner_app: p.app_id, owner_user: doc.actor_id, created_at_ms: at }]);
    return finish('conflict', { ...transcript, changed_files: [path], patch: { ref, file_id }, error: { status: 'failed', code: 'conflict', reason: 'draft_changed' } });
  }
  async function runCodingCancel(doc) {
    const turn = await turnNow(doc.payload.turn_id);
    if (!turn || !['queued', 'running'].includes(turn.state)) return { status: 'failed', error: 'conflict: turn_not_running' };
    await setTurn(turn.id, { state: 'cancelled', finished_at_ms: Date.now() }); // zuerst persistieren, dann „Sidecar beenden“
    activeTurns.delete(turn.draft_id);
    return { status: 'done', progress: 1, result: { turn_id: turn.id, state: 'cancelled' } };
  }

  // ---- Befehle (§8): Dokument in business_commands, Status gepusht ----
  // K-WJ-B2: veröffentlichte Befehls-Registry des simulierten Cores = Plattformbefehle ctox.coding.*, per sim.onCommand
  // bzw. sim.failCommand registrierte Typen (aus jedem Tab) und die handlers der aktiven Version der sendenden App.
  function routingManifest(appId) {
    const app=mirrorGet('business_apps',appId);
    if(app?.revoked||app?.base?.deleted)return undefined;
    const ver=app?.base?.data?.active_ver;
    const row=ver?mirrorGet('business_app_versions',`${appId}@${ver}`):null;
    return !row?.revoked&&!row?.base?.deleted?row?.base?.data?.manifest:undefined;
  }
  function commandRouteKnown(appId,type) {
    return commandRoute({app_id:appId,type},routingManifest,null,sim0.cmdHandlers.has(type)||sim0.cmdTypes.has(type)||sim0.failCmd.has(type)).error!=='unknown_command';
  }
  async function runCommand(doc) {
    const id = doc.id, type = doc.type;
    if (type === 'ctox.coding.turn' || type === 'ctox.coding.cancel') {
      await sleep(latencyMs);
      await coreMerge('business_commands', id, { status: 'accepted' });
      await coreMerge('business_commands', id, { status: 'running' });
      let fin;
      try { fin = type === 'ctox.coding.turn' ? await runCodingTurn(doc) : await runCodingCancel(doc); } catch (err) { fin = { status: 'failed', error: String(err?.message ?? err) }; }
      await coreMerge('business_commands', id, fin);
      return;
    }
    await sleep(latencyMs);
    await coreMerge('business_commands', id, { status: 'accepted' });
    await sleep(latencyMs);
    await coreMerge('business_commands', id, { status: 'running', progress: 0.5 });
    let patch;
    const fail = sim0.failCmd.get(type);
    if (fail !== undefined) {
      sim0.failCmd.delete(type);
      patch = { status: 'failed', error: fail };
    } else {
      const h = sim0.cmdHandlers.get(type);
      try {
        // sim.onCommand-Handler bekommen wie ein App-Handler ctx.model (#830) mit derselben simulierten Antwort
        const route=commandRoute(doc,routingManifest,actor,!!h);
        if(route.error)throw new CtoxError(route.error,route.error);
        const manifest=routingManifest(route.target);
        const path=manifest?.handlers?.[type];
        const artifact=path ? await readArtifactContent(manifest.files[path]) : null;
        const handler=artifact===null?null:(await import(`data:text/javascript;base64,${btoa(String.fromCharCode(...new TextEncoder().encode(typeof artifact==='string'?artifact:await artifact.text())))}`)).default;
        const result = !route.platform && handler ? await handler({...cloneValue(doc),actor:{id:actor.id,role:actor.role}}, {app_id:route.target}) : !route.platform && h ? await h(cloneValue(doc.payload), { app_id: doc.app_id, command_id: id, model: (req) => Promise.resolve().then(() => simulatedModelRun(doc.app_id, req)),
          cap: { process: { run: capProcessRun(doc.app_id) } } }) : { ok: true }; // CAP-3d process.run (simuliert)
        patch = { status: 'done', progress: 1, result: result ?? null };
      } catch (err) { patch = { status: 'failed', error: String(err?.message ?? err) }; }
    }
    await sleep(latencyMs);
    await coreMerge('business_commands', id, patch);
  }

  // ---- App-Versionen, Generationen, Aktivierung (§9.3, §16.4) ----
  function versionDoc(appId, ver) { return visible(mirrorGet('business_app_versions', `${appId}@${ver}`)); }
  function genOf(ns) { const m = /@g(\d+)$/.exec(ns); return m ? Number(m[1]) : 1; }
  function resolveNs(app, collection) {
    if (SYSTEM_COLLECTIONS.has(collection) || collection.includes('@g')) return collection;
    const manifest = versionDoc(app.id, app.ver)?.manifest;
    const foreign = manifest?.foreign_collections?.[collection];
    const owner = foreign?.owner_app ?? foreign?.app ?? foreign?.owner;
    const ownerVer = owner && (visible(mirrorGet('business_apps',owner))?.active_ver ?? activations.get(owner)?.activated_ver);
    const g = (owner ? versionDoc(owner,ownerVer)?.manifest : manifest)?.generations?.[collection] ?? 1;
    return nsName(collection, g);
  }
  function normalizeLayout(l) {
    const out = {};
    for (const k of LAYOUT_KEYS) out[k] = l?.[k] ?? null;
    return out;
  }
  // §16.19 Nr. 3: shell_preferences (Eigentum ctox-platform) – eine Zeile je Nutzer, sichtbar nur die eigene.
  // §16.22: app_drafts (Eigentum ctox-platform) – Zeile je (App, Nutzer), id = '<user>/<app>', nur eigener Nutzer.
  function ownRowOk(ns, id) {
    if (ns === 'shell_preferences') return id === actor.id;
    if (ns === 'app_drafts') return typeof id === 'string' && id.startsWith(`${actor.id}/`);
    return true;
  }
  function schemaFor(app, collection) { return versionDoc(app.id, app.ver)?.manifest?.collections?.[collection]?.schema ?? null; }

  async function publishInternal(appId, ver, o = {}) {
    if (publishing.has(appId)) throw new CtoxError('conflict', `Veröffentlichung von ${appId} läuft bereits (busy)`);
    publishing.add(appId);
    try {
      const appDoc = visible(mirrorGet('business_apps', appId));
      if (o.baseAppVer !== undefined && (appDoc?.active_ver ?? null) !== o.baseAppVer) throw new CtoxError('conflict', 'stale_base');
      if (versionDoc(appId, ver)) throw new CtoxError('conflict', `Version ${appId}@${ver} existiert bereits`);
      const prevManifest = appDoc ? versionDoc(appId, appDoc.active_ver)?.manifest : null;
      const files = o.files ?? {};
      const fileHashes = {};
      for (const [path, content] of Object.entries(files)) {
        const text = typeof content === 'string' ? content : String(content);
        const hash = await sha256Hex(text);
        fileHashes[path] = hash;
        await db.tx((t) => t.put('sim_core', { key: `file:${hash}`, content: text }));
      }
      const generations = { ...(prevManifest?.generations ?? {}) };
      const retires = {};
      for (const c of o.incompatible ?? []) {
        const oldGen = generations[c] ?? 1;
        retires[c] = nsName(c, oldGen);
        const newGen = oldGen + 1;
        generations[c] = newGen;
        const oldNs = nsName(c, oldGen), newNs = nsName(c, newGen);
        const fn = o.migrate?.[c];
        const migrate = typeof fn === 'function' ? fn : (d) => d;
        doubleWrite.set(oldNs, { ns_new: newNs, migrate }); // parallele Writes auf g<k> werden doppelt geschrieben
        const src = (await db.all('docs')).filter((r) => r.collection === oldNs && r.base?.data && !r.base.deleted);
        const migrated = src.map((r) => ({ ...migrate(cloneValue(r.base.data)), id: r.id }));
        if (migrated.length) await corePush(newNs, migrated);
      }
      const head = (await db.get('sim_core', 'core'))?.head ?? 0;
      const M = o.migratedAtSeq ?? head;
      const manifest = {
        app_id: appId, app_ver: ver, files: fileHashes, entry: o.entry ?? 'index.js',
        collections: o.collections ?? prevManifest?.collections ?? {}, generations,
        retires, handlers: o.handlers ?? {}, min_app_api: o.minAppApi ?? '4.0',
        layout: normalizeLayout(o.layout ?? prevManifest?.layout), styles: [...(o.styles ?? prevManifest?.styles ?? [])],
        capabilities_required: o.capabilitiesRequired ?? [],
        provenance: { base_release: null, base_hash: null, locally_modified: !!o.locallyModified },
        ...(o.manifestExtra ?? {}),
        author: actor.id, created_at: new Date().toISOString(),
      };
      await corePush('business_app_versions', [{ id: `${appId}@${ver}`, app_id: appId, app_ver: ver, manifest, migrated_at_seq: M }]);
      await corePush('business_apps', [{ ...(appDoc ?? {}), id: appId, title: o.title ?? appDoc?.title ?? appId, active_ver: ver, migrated_at_seq: M }]);
      return { app_id: appId, app_ver: ver, migrated_at_seq: M };
    } finally { publishing.delete(appId); }
  }
  function scheduleActivation(appId) {
    if (!isLeader || closed) return;
    const prev = activationChains.get(appId) ?? Promise.resolve();
    const next = prev.then(() => runActivation(appId)).catch((err) => {
      if (err instanceof StaleLeader) stepDown();
      else if (!closed) console.error('[ctox-stub] activation', err);
    });
    activationChains.set(appId, next);
    track(next);
  }
  function isTerminalFor(appId, ver) {
    const a = activations.get(appId);
    return a && a.target_ver === ver && (a.state === 'activated' || a.state === 'failed');
  }
  async function runActivation(appId) {
    const appDoc = visible(mirrorGet('business_apps', appId));
    if (!appDoc?.active_ver || !isLeader) return;
    const ver = appDoc.active_ver;
    if (isTerminalFor(appId, ver)) return;
    const vdoc = versionDoc(appId, ver);
    if (!vdoc) return; // Manifest noch nicht da – Ankunft löst erneut aus
    const manifest = vdoc.manifest;
    const superseded = () => visible(mirrorGet('business_apps', appId))?.active_ver !== ver;
    await setActivation(appId, { target_ver: ver, state: 'received', error: null });
    // fetched: alle Dateien per Hash geprüft (aus „Spur 4“ = sim_core-CAS)
    for (const [path, hash] of Object.entries(manifest.files ?? {})) {
      if (await db.get('artifacts', hash)) continue;
      await waitUntil(() => link.state === 'online' || closed);
      // §16.15a Nr. 3: sha256-Fehlschlag → einmal neu über Spur 4 holen, erst wiederholter Fehlschlag → failed
      let content = null;
      for (let attempt = 0; attempt < 2 && content === null; attempt++) {
        const src = await db.get('sim_core', `file:${hash}`);
        if (!src) { await setActivation(appId, { target_ver: ver, state: 'failed', error: `artifact_missing:${path}` }); return; }
        let got = src.content;
        if (corruptNextFetch > 0) { corruptNextFetch--; got = `${got}\n/* beschädigt */`; }
        if ((await sha256Hex(got)) === hash) content = got;
        else simRecord.refetches++;
      }
      if (content === null) { await setActivation(appId, { target_ver: ver, state: 'failed', error: `hash_mismatch:${path}` }); return; }
      await db.tx((t) => t.put('artifacts', { hash, content, app_id: appId, app_ver: ver, path, stored_at: Date.now() }));
    }
    await setActivation(appId, { target_ver: ver, state: 'fetched' });
    const missing = (manifest.capabilities_required ?? []).filter((c) => !capabilities.has(c));
    if (missing.length || compareVer(manifest.min_app_api ?? '4.0', APP_API_VERSION) > 0) {
      await setActivation(appId, { target_ver: ver, state: 'failed', error: `needs_update:${missing.join(',') || `app_api ${manifest.min_app_api}`}` });
      return;
    }
    const M = vdoc.migrated_at_seq ?? appDoc.migrated_at_seq ?? 0;
    await waitUntil(() => checkpoint >= M || superseded() || closed || !isLeader);
    if (superseded() || closed || !isLeader) return;
    await setActivation(appId, { target_ver: ver, state: 'data-ready' });
    const prevAct = activations.get(appId);
    const good = [...(prevAct?.good_vers ?? []).filter((v) => v !== ver), ver].slice(-10);
    for (const c of Object.keys(manifest.collections ?? {})) subscribeNs(nsName(c, manifest.generations?.[c] ?? 1)); // §16.45
    await setActivation(appId, { target_ver: ver, activated_ver: ver, previous_ver: prevAct?.activated_ver ?? null, good_vers: good, state: 'activated', at_seq: checkpoint });
    // §16.16: alte Generation(en) erst verwerfen, wenn die neue Version 10 min ohne Rückfall lief
    const retires = Object.values(manifest.retires ?? {});
    if (retires.length) later(() => track(retireGenerations(appId, ver, retires)), generationGraceMs);
  }
  async function retireGenerations(appId, ver, namespaces) {
    if (!isLeader || activations.get(appId)?.activated_ver !== ver) return;
    const dropped = await db.tx(async (t) => {
      await assertGen(t);
      const keys = [];
      for (const r of await t.all('docs')) if (namespaces.includes(r.collection)) { await t.del('docs', [r.collection, r.id]); keys.push([r.collection, r.id]); }
      await bumpLocalSeq(t);
      return keys;
    });
    dropNamespaces(namespaces);
    for (const ns of namespaces) doubleWrite.delete(ns);
    post({ type: 'drop_ns', namespaces });
    return dropped.length;
  }
  function dropNamespaces(namespaces) {
    for (const ns of namespaces) if (mirror.delete(ns)) notify(`c:${ns}`);
  }
  function compareVer(a, b) {
    const pa = String(a).split('.').map(Number), pb = String(b).split('.').map(Number);
    for (let i = 0; i < Math.max(pa.length, pb.length); i++) { const d = (pa[i] ?? 0) - (pb[i] ?? 0); if (d) return d; }
    return 0;
  }
  async function setActivation(appId, patch) {
    const value = await db.tx(async (t) => {
      await assertGen(t);
      const cur = (await t.get('meta', `activation:${appId}`)) ?? { key: `activation:${appId}`, app_id: appId, activated_ver: null, target_ver: null, state: null, error: null };
      const v = { ...cur, ...patch, updated_at: Date.now() };
      await t.put('meta', v);
      await bumpLocalSeq(t);
      return v;
    });
    simRecord.activationReports.push({ app_id: appId, app_ver: value.target_ver, state: value.state, at_seq: checkpoint, error: value.error ?? undefined });
    applyActivationLocal(value, true);
    post({ type: 'activation', value });
    return value;
  }
  function applyActivationLocal(value, fromSelf) {
    const prev = activations.get(value.app_id);
    if (prev && prev.updated_at > value.updated_at) return;
    if (prev && jsonEq(prev, value)) return;
    activations.set(value.app_id, value);
    notify('activation');
    const ev = Object.freeze({ app_id: value.app_id, ver: value.target_ver, state: value.state, activated_ver: value.activated_ver, error: value.error ?? null });
    for (const cb of [...activationListeners]) { try { cb(ev); } catch (e) { console.error('[ctox-stub] activation listener', e); } }
    if (value.state === 'failed' && value.error?.startsWith('needs_update')) {
      zoneReport(value.app_id, { level: 'error', code: 'app.needs_update', text: `App ${value.app_id}@${value.target_ver} benötigt ein Update dieses Teilnehmers (${value.error.slice(13)})`, ref: value.target_ver }, 'kernel');
    } else if (value.state === 'failed' && value.error) {
      zoneReport(value.app_id, { level: 'error', code: 'app.activation_failed', text: `Aktivierung ${value.app_id}@${value.target_ver} fehlgeschlagen: ${value.error}`, ref: value.target_ver }, 'kernel');
    }
    void fromSelf;
  }

  // ---- Hilfen für den Shell-Tausch (Mount/Remount von Apps macht die Shell, §16.19 Nr. 1; siehe testkit/) ----
  function withTimeout(p, ms, onLate) {
    return new Promise((resolve, reject) => {
      let timedOut = false;
      const t = later(() => { timedOut = true; reject(new CtoxError('unavailable', `mount nicht binnen ${ms} ms`)); }, ms);
      p.then((v) => { if (timedOut) onLate?.(v); else { clearTimeout(t); timers.delete(t); resolve(v); } },
        (e) => { if (!timedOut) { clearTimeout(t); timers.delete(t); reject(e); } });
    });
  }
  function checkSnapshot(appId, snap) {
    if (snap === undefined) return undefined;
    try {
      const c = structuredClone(snap);
      let size = 0;
      try { size = JSON.stringify(c)?.length ?? 0; } catch { size = 0; }
      if (size > SNAPSHOT_MAX_BYTES) throw new CtoxError('quota', 'Snapshot > 1 MiB');
      return c;
    } catch (e) {
      zoneReport(appId, { level: 'warn', code: 'app.snapshot_invalid', text: `Snapshot verworfen: ${e?.message ?? e}` }, 'kernel');
      return undefined;
    }
  }
  // ---- Datei-Blobs ----
  function setUploadProgress(ref, sent, total, broadcast) {
    uploadProgress.set(ref, { sent, total });
    notify(`upload:${ref}`);
    if (broadcast) post({ type: 'upload', ref, sent, total });
  }

  // ---- App-API (§10.2 + §10.2a + §16.10) ----
  function makeAppApi(appId, appVer) {
    if (typeof appId !== 'string' || !appId) throw new CtoxError('invalid', 'appId fehlt');
    const app = Object.freeze({ id: appId, ver: String(appVer ?? activations.get(appId)?.activated_ver ?? '0') });
    const win = windowStates.get(appId) ?? { title: appId, header: null, size: { width: 0, height: 0 }, mode: 'normal', closed: false };
    windowStates.set(appId, win);
    const winTag = `win:${appId}`;
    const shell = Object.freeze({
      setTitle(title) { win.title = String(title); notify(winTag); },
      // §16.19 Nr. 5: App rendert ihr Kopfzeilen-Element, die Shell setzt es in den Kopf-Slot (Referenz, keine Kopie).
      registerHeader(el) {
        if (el !== null && (typeof el !== 'object' || (typeof HTMLElement !== 'undefined' && !(el instanceof HTMLElement)))) {
          throw new CtoxError('invalid', 'registerHeader braucht ein HTMLElement (oder null)', { path: 'el' });
        }
        win.header = el;
        notify(winTag);
      },
      canNavigate(target, params = {}) {
        const known = target === 'shell' || !!visible(mirrorGet('business_apps', target));
        if(!known)return false;
        if(params.command_type!==undefined)return (readManifest(appId,app.ver)?.commands_out??[]).some(x=>x.app===target&&x.type===params.command_type);
        return true;
      },
      navigate(target, params = {}) {
        const known = shell.canNavigate(target,params);
        if (!known) {
          zoneReport(appId, { level: 'error', code: 'shell.navigate.unknown_target', text: `Navigationsziel unbekannt: ${target}`, ref: String(target) }, 'kernel');
          return false;
        }
        const ev = Object.freeze({ from: appId, to: target, params: cloneValue(params) });
        simRecord.navigations.push(ev);
        for (const cb of [...(routeListeners.get(target) ?? [])]) { try { cb(ev); } catch (e) { console.error(e); } }
        return true;
      },
      onRoute(cb) {
        if (!routeListeners.has(appId)) routeListeners.set(appId, new Set());
        routeListeners.get(appId).add(cb);
        return () => routeListeners.get(appId)?.delete(cb);
      },
      locale: makeLive(['locale'], () => locale),
      t: (key) => translations[locale]?.[key] ?? key,
      theme: makeLive(['theme'], () => ({ ...theme })),
      dialog(spec) { simRecord.dialogs.push({ app_id: appId, kind: 'dialog', spec: cloneValue(spec) }); return Promise.resolve(spec?.default ?? null); },
      menu(spec) { simRecord.dialogs.push({ app_id: appId, kind: 'menu', spec: cloneValue(spec) }); return Promise.resolve(spec?.default ?? null); },
      window: Object.freeze({
        size: makeLive([winTag], () => ({ ...win.size })),
        mode: makeLive([winTag], () => win.mode),
        close() { win.closed = true; notify(winTag); },
      }),
      openFile(ref) { simRecord.openedFiles.push({ app_id: appId, ref }); },
    });
    const legacyFiles = Object.freeze({
      async get(ref) {
        guardAlive();
        const local = await db.get('blobs', ref);
        if (local) {
          await db.tx((t) => t.put('blobs', { ...local, last_used: Date.now() }));
          return local.blob;
        }
        const net = isLeader ? link : followerNet?.link ?? link;
        if (net.state !== 'online') throw new CtoxError('unavailable', 'Datei nicht lokal und offline');
        const core = await db.get('sim_core', `blob:${ref}`);
        if (!core) throw new CtoxError('not_found', `Datei ${ref} unbekannt`);
        await sleep(latencyMs);
        if ((await sha256Hex(core.blob)) !== ref) throw new CtoxError('invalid', 'Prüfsumme stimmt nicht (Empfang verworfen)');
        await db.tx((t) => t.put('blobs', { ref, blob: core.blob, size: core.size, type: core.type, pinned: false, last_used: Date.now() }));
        return core.blob;
      },
      put(blob, meta = {}) {
        let metaHandle = null, uploadOp = null, cancelledEarly = false, refValue = null, resolveResult;
        const result = new Promise((r) => { resolveResult = r; });
        const progressTag = () => `upload:${refValue}`;
        const progress = {
          get: () => (refValue ? uploadProgress.get(refValue) ?? { sent: 0, total: blob?.size ?? 0 } : { sent: 0, total: blob?.size ?? 0 }),
          subscribe(cb) {
            let off = null, stop = false;
            refP.then(() => { if (!stop) off = makeLive([progressTag()], () => progress.get()).subscribe(cb); }, () => {});
            cb(progress.get());
            return () => { stop = true; off?.(); };
          },
        };
        const refP = (async () => {
          guardAlive();
          if (!isBlob(blob)) throw new CtoxError('invalid', 'files.put braucht einen Blob', { path: 'blob' });
          refValue = await sha256Hex(blob);
          return refValue;
        })();
        refP.catch(() => {});
        const local = (async () => {
          const ref = await refP;
          if (!loaded) await new Promise((r) => preReady.push(r));
          guardAlive();
          if (cancelledEarly) return;
          const id = meta.id ?? uuidv7();
          const op_id = uuidv7();uploadOp=op_id;
          setUploadProgress(ref, 0, blob.size, false);
          await db.tx(async (t) => {
            const existing = await t.get('blobs', ref);
            await t.put('blobs', { ref, blob, size: blob.size, type: blob.type, pinned: true, last_used: Date.now(), ...(existing ? {} : {}) });
            await t.put('outbox', { op_id, collection: '__blobs', id: ref, op: 'upload', body: null, file_id:id, app_id: appId, app_ver: app.ver, created_at: Date.now(), origin_tab: tabId });
            await t.put('meta',{key:`file-owner:${JSON.stringify([appId,id])}`,ref});
          });
          post({ type: 'outbox' });
          if (isLeader) schedulePump();
          metaHandle = localWrite(app, 'put', 'files', id, {
            id, name: String(meta.name ?? 'datei'), mime: String(meta.mime ?? (blob.type || 'application/octet-stream')),
            size: blob.size, ref, owner_app: appId,
          }, false, {file_upload:true,file_upload_op:uploadOp});
          await metaHandle.local;
        })();
        local.then(async () => {
          if (cancelledEarly || !metaHandle) { resolveResult({ status: 'rejected', reason: 'cancelled' }); return; }
          const r = await metaHandle.result;
          if (r.status !== 'rejected') await waitUntil(() => uploadResults.has(uploadOp) || closed);
          const upload = uploadResults.get(uploadOp);
          const final = r.status === 'rejected' ? r : upload?.status === 'accepted' ? r : {status:'rejected',code:upload?.code ?? 'unavailable',reason:upload?.reason ?? 'Upload not confirmed before close'};
          resolveResult({ ...final, ref: refValue });
        }, (err) => resolveResult({ status: 'rejected', reason: err instanceof CtoxError ? err.code : 'local_error' }));
        local.catch(() => {});
        const done = result.then(r => {
          if (r.status === 'rejected') throw new CtoxError(r.code ?? r.reason ?? 'unavailable', r.reason ?? 'File rejected', {reason:r.reason});
          return r;
        });
        done.catch(() => {}); // consumers may attach after the local acknowledgement
        return {
          local, result, ref: refP, done,
          upload: Object.freeze({ progress }),
          cancel() {
            if (!metaHandle) { cancelledEarly = true; return true; }
            return metaHandle.cancel();
          },
        };
      },
    });
    // ---- K-1g: files nach CAP-3b (#844) – Ergebnisse immer Objekte, nie Base64, nur Dateien der eigenen App ----
    const fileDocs = () => [...(mirror.get('files')?.values() ?? [])].map((r) => visible(r)).filter(Boolean);
    function ownFile(sel) {
      if (!isPlainObject(sel) || (typeof sel.id !== 'string' && typeof sel.ref !== 'string')) throw new CtoxError('invalid', 'files: {id} oder {ref} nötig', { path: 'ref' });
      const docs = fileDocs().filter((d) => (sel.id ? d.id === sel.id : d.ref === sel.ref));
      const mine = docs.find((d) => d.owner_app === appId);
      if (!mine && docs.length) throw new CtoxError('forbidden', 'Datei gehört einer anderen App', { path: sel.id ? 'id' : 'ref' });
      return mine ?? null;
    }
    // Grants come from Core-confirmed manifests, never optimistic metadata.
    const readManifest = (id, ver) => {
      const row = mirrorGet('business_app_versions',`${id}@${ver}`);
      return !row?.revoked && !row?.base?.deleted ? row?.base?.data?.manifest : undefined;
    };
    const currentManifest = owner => {
      const row = mirrorGet('business_apps',owner);
      const ver = !row?.revoked && !row?.base?.deleted ? row?.base?.data?.active_ver : undefined;
      return readManifest(owner,ver);
    };
    const ownReferenceRows = new Set(['shell_preferences','app_drafts']);
    function readableNamespace(ns) {
      const caller = readManifest(appId,app.ver);
      const c = ns.replace(/@g[1-9][0-9]*$/,'');
      if (caller?.collections?.[c]) return ns === resolveNs(app,c);
      const f = caller?.foreign_collections?.[c];
      const owner = f?.owner_app ?? f?.app ?? f?.owner;
      const om = owner && currentManifest(owner);
      return !!(f?.ops?.includes('read') && om?.collections?.[c] && om?.exports?.[c]?.includes('read') && ns === nsName(c,om.generations?.[c] ?? 1));
    }
    // §16.13: known owners and declared foreign reads require both grants.
    // Published callers fail closed on undeclared names, even before the owner
    // manifest arrives. Only manifest-less contract/bootstrap names are local.
    function assertReadableCollection(collection) {
      const c = collection.replace(/@g[1-9][0-9]*$/, '');
      const callerManifest = readManifest(appId,app.ver);
      const foreign = callerManifest?.foreign_collections?.[c];
      const owners = [...(mirror.get('business_app_versions')?.values() ?? [])]
        .filter(row => !row.revoked && !row.base?.deleted).map(row => row.base?.data).filter(Boolean).filter(v => v.app_id !== appId && v.manifest?.collections?.[c]);
      if ((foreign || owners.length || (callerManifest && !callerManifest.collections?.[c])) && !readableNamespace(resolveNs(app, collection)))
        throw new CtoxError('forbidden', 'Foreign collection requires declaration and read export', {path:'collection'});
    }
    function readableLive(collection, tags, compute, empty) {
      const live = makeLive([...tags,'c:business_apps','c:business_app_versions'], () => {
        try { assertReadableCollection(collection); }
        catch (error) { if (error.code === 'forbidden') return empty; throw error; }
        return compute();
      });
      return {
        get() { assertReadableCollection(collection); return live.get(); },
        subscribe(cb) { assertReadableCollection(collection); return live.subscribe(cb); },
      };
    }
    // E30: a CAS hash is not authority. Only canonical, still-readable rows
    // grant access; an optimistic reference cannot mint a read grant.
    async function readableFile(sel) {
      guardAlive();
      if (!isPlainObject(sel) || (typeof sel.id !== 'string' && typeof sel.ref !== 'string')) throw new CtoxError('invalid', 'files: {id} oder {ref} nötig', {path:'ref'});
      const docs = fileDocs().filter(d => sel.id ? d.id === sel.id : d.ref === sel.ref);
      for (const d of docs) {
        if (d.owner_app !== appId) continue;
        const row = mirrorGet('files',d.id), base = row?.base;
        if (row?.revoked) continue;
        if (!base?.deleted && base?.data?.owner_app === appId && base.data.ref === d.ref) return d;
        // files.put proves local bytes in the blob transaction, durably across
        // reload. A plain optimistic files row cannot claim another app's CAS.
        if ((row?.pending || row?.local_file_rejection) && (await db.get('meta',`file-owner:${JSON.stringify([appId,d.id])}`))?.ref === d.ref) return d;
      }
      const canonicalRows = [];
      for (const [ns, rows] of mirror) {
        const declared = readableNamespace(ns);
        if (!declared && ns !== 'business_commands' && !ownReferenceRows.has(ns)) continue;
        for (const row of rows.values()) if (!row.deleted && !row.revoked && !row.base?.deleted && row.base?.data && ownRowOk(ns,row.id)) {
          const d = row.base.data;
          // Standard API handles already expose own command results and own
          // platform rows; they need no foreign-collection declaration.
          if (declared || ownReferenceRows.has(ns) || (ns === 'business_commands' && d.app_id === appId && d.actor_id === userId)) canonicalRows.push([ns,d]);
        }
      }
      const desktop = canonicalRows.find(([ns,d]) => ns.replace(/@g[1-9][0-9]*$/,'') === 'desktop_files' && d.id === sel.id && !d.is_deleted)?.[1];
      if (desktop) {
        const ref = [desktop.ref,desktop.content_ref,docs[0]?.ref].find(v => typeof v === 'string' && /^[a-f0-9]{64}$/.test(v));
        if (typeof ref !== 'string' || !/^[a-f0-9]{64}$/.test(ref)) throw new CtoxError('unavailable','Desktop file has no CAS reference',{path:'id'});
        const meta = fileDocs().find(d => d.ref === ref);
        return {...meta,id:desktop.id,ref,name:desktop.name ?? meta?.name,mime:desktop.mime ?? desktop.mime_type ?? meta?.mime,size:undefined, verify_size:true};
      }
      const ref = docs[0]?.ref ?? sel.ref;
      const containsRef = value => value !== null && typeof value === 'object' && (value.ref === ref || value.blob_ref === ref || Object.values(value).some(containsRef));
      if (typeof ref === 'string' && /^[a-f0-9]{64}$/.test(ref) && canonicalRows.some(([,d]) => containsRef(d))) return docs[0] ?? {ref,id:ref,name:null};
      throw new CtoxError('forbidden','File needs ownership or a readable reference',{path:sel.id ? 'id' : 'ref'});
    }
    const fileInfo = (d) => ({ id: d.id, ref: d.ref, name: d.name ?? null, mime: d.mime ?? 'application/octet-stream', size: d.size ?? 0 });
    const files = Object.freeze({
      /** CAP-3b: put(bytes | {text}, {name, mime}) → {id, ref}. Blob = Altform (Handle, @deprecated). */
      put(content, meta = {}) {
        if (isBlob(content)) return legacyFiles.put(content, meta);
        return (async () => {
          guardAlive();
          let bytes, text = false;
          if (isPlainObject(content) && typeof content.text === 'string') { bytes = new TextEncoder().encode(content.text); text = true; }
          else if (content instanceof ArrayBuffer) bytes = new Uint8Array(content.slice(0));
          else if (ArrayBuffer.isView(content)) bytes = new Uint8Array(content.buffer.slice(content.byteOffset, content.byteOffset + content.byteLength));
          else throw new CtoxError('invalid', 'files.put: Inhalt muss Uint8Array/ArrayBuffer oder {text} sein', { path: 'content' });
          const mime = String(meta.mime ?? (text ? 'text/plain; charset=utf-8' : 'application/octet-stream'));
          const id = typeof meta.id === 'string' && meta.id ? meta.id : uuidv7();
          const h = legacyFiles.put(new Blob([bytes], { type: mime }), { ...meta, id, mime, name: meta.name ?? 'datei' });
          const ref = await h.ref;
          await h.local;
          return Object.defineProperty({id,ref},'done',{value:h.done});
        })();
      },
      /** CAP-3b: get({id|ref}) → {found, id, ref, name, mime, size, text|bytes}; Text nur für UTF-8 ≤ 512 KiB. String = Altform (Blob). */
      async get(sel) {
        if (typeof sel === 'string') return legacyFiles.get((await readableFile({ref:sel})).ref);
        guardAlive();
        let d = await readableFile(sel);
        if (sel.range !== undefined) {
          const blob = await legacyFiles.get(d.ref);
          if (await sha256Hex(blob) !== d.ref) throw new CtoxError('invalid','File content hash mismatch');
          d = {...d,size:blob.size};
        }
        if (!d) return { found: false, ref: sel.ref ?? null, ...(sel.id ? { id: sel.id } : {}) };
        if (sel.range !== undefined) {
          const {start, end} = sel.range ?? {};
          if (!isPlainObject(sel.range) || !Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || end < start || end > d.size) throw new CtoxError('invalid', 'Byte range outside file', {path:'range'});
          const blob = await legacyFiles.get(d.ref);
          if (await sha256Hex(blob) !== d.ref) throw new CtoxError('invalid', 'File content hash mismatch');
          const bytes = new Uint8Array(await blob.slice(start, end).arrayBuffer());
          return {found:true, ...fileInfo(d), bytes};
        }
        const bytes = new Uint8Array(await (await legacyFiles.get(d.ref)).arrayBuffer());
        let text;
        if (bytes.byteLength <= 512 * 1024) { try { text = new TextDecoder('utf-8', { fatal: true }).decode(bytes); } catch { text = undefined; } }
        return { found: true, ...fileInfo(d), size: bytes.byteLength, ...(text !== undefined ? { text } : { bytes }) };
      },
      async list() { guardAlive(); return { files: fileDocs().filter((d) => d.owner_app === appId).map(fileInfo).sort((a, b) => (a.id < b.id ? -1 : 1)) }; },
      async delete(sel) {
        guardAlive();
        const d = ownFile(sel);
        if (!d) return { deleted: false };
        const ok = await localWrite(app, 'delete', 'files', d.id, null).local.then(() => true, () => false);
        return { deleted: ok, id: d.id };
      },
    });
    // ---- K-1g: model.run (#830 / CATALOG §6) – deterministische simulierte Antwort, nur mit deklariertem model ----
    const model = Object.freeze({
      async run(req) {
        guardAlive();
        const declared = versionDoc(appId, app.ver)?.manifest?.capabilities_required ?? [];
        if (!declared.includes('model') && !declared.includes('model.run')) throw new CtoxError('forbidden', 'model nicht in capabilities_required', { path: 'model' });
        return simulatedModelRun(appId, req);
      },
    });
    const actorApi = Object.freeze({
      get id() { return actor.id; },
      get display_name() { return actor.display_name; },
      get role() { return actor.role; },
      permissions: makeLive(['actor'], () => [...actor.permissions]),
    });
    return Object.freeze({
      version: APP_API_VERSION,
      app,
      query(collection, q = {}) {
        assertReadableCollection(collection);
        const cq = compileQuery(q);
        const ns = resolveNs(app, collection);
        subscribeNs(ns);
        return readableLive(collection, [`c:${ns}`], () => {
          const m = mirror.get(ns);
          if (!m) return [];
          const docs = [];
          for (const r of m.values()) if (r.data != null && !r.deleted && ownRowOk(ns, r.id)) docs.push(r.data);
          return runCompiledQuery(docs, cq);
        }, []);
      },
      get(collection, id) {
        assertReadableCollection(collection);
        const ns = resolveNs(app, collection);
        subscribeNs(ns);
        return readableLive(collection, [`c:${ns}`, 'actor'], () => ownRowOk(ns,id) ? visible(mirrorGet(ns,id)) : null, null);
      },
      put(collection, doc) { return localWrite(app, 'put', collection, doc?.id, doc); },
      patch(collection, id, patch, options = {}) { return localWrite(app, 'patch', collection, id, patch, false, {expected_revision:options.expected_revision}); },
      remove(collection, id) { return localWrite(app, 'delete', collection, id, null); },
      command(type, payload = {}, cmdOpts = {}) {
        if (typeof type !== 'string' || !type) throw new CtoxError('invalid', 'Befehlstyp fehlt', { path: 'type' });
        const route=commandTarget({type,app_id:appId,...(cmdOpts.target_app===undefined?{}:{target_app:cmdOpts.target_app})},readManifest(appId,app.ver));
        const key = cmdOpts.idempotency_key;
        if (key !== undefined && (typeof key !== 'string' || key.length > 128)) throw new CtoxError('invalid', 'idempotency_key must be a string of at most 128 characters', { path: 'idempotency_key' });
        // UUIDv8: first 128 SHA-256 bits, RFC variant/version bits normalized.
        const hash = key === undefined ? null : sha256Js(new TextEncoder().encode(`${appId}|${actor.id}|${key}`));
        const id = hash === null ? uuidv7() : `${hash.slice(0,8)}-${hash.slice(8,12)}-8${hash.slice(13,16)}-${((parseInt(hash[16],16)&3)|8).toString(16)}${hash.slice(17,20)}-${hash.slice(20,32)}`;
        const write = localWrite(app, 'put', 'business_commands', id, {
          id, type, payload, status: 'queued', app_id: appId, actor_id: actor.id, created_at: new Date().toISOString(),
          ...(cmdOpts.target_app!==undefined ? {target_app:cmdOpts.target_app} : !route.error && !route.platform && route.target!==appId ? {target_app:route.target} : {}),
          ...(cmdOpts.record_id !== undefined ? { record_id: String(cmdOpts.record_id) } : {}), // §16.19 Nr. 8
          // §16.39 Nr. 3: Rückruf-Befehlstypen der auslösenden App am Befehlsdokument (Core-PR #848 liest sie dort)
          ...(typeof cmdOpts.on_complete === 'string' ? { on_complete: cmdOpts.on_complete } : {}),
          ...(typeof cmdOpts.on_failed === 'string' ? { on_failed: cmdOpts.on_failed } : {}),
        }, key !== undefined);
        const status = makeLive(['c:business_commands', 'commands'], () => {
          const d = visible(mirrorGet('business_commands', id));
          if (!d) {
            if (cmdRejected.has(id)) return { status: 'failed', error: cmdRejected.get(id) };
            return { status: 'queued' };
          }
          const s = { status: d.status };
          if (d.progress !== undefined) s.progress = d.progress;
          if (d.result !== undefined) s.result = d.result;
          if (d.error !== undefined) s.error = d.error;
          return s;
        });
        write.result.then((r) => { if (r.status === 'rejected') { cmdRejected.set(id, r.reason); notify('commands'); } });
        return Object.freeze({ id, status, local: write.local, result: write.result, cancel: write.cancel });
      },
      setVisible(collections) {
        simRecord.lastVisible = Object.freeze({ app_id: appId, collections: [...(collections ?? [])].map((c) => resolveNs(app, c)), at: Date.now() });
      },
      status: Object.freeze({
        report(item) { zoneReport(appId, item, 'app'); },
        clear(code) { zoneClear(appId, code); },
      }),
      metrics: metricsLive,
      shell,
      actor: actorApi,
      files,
      model,
    });
  }

  // ---- Shell-Kernel-API (§10.2a Nr. 6–8) ----
  function hasPerm(p) { return actor.permissions.includes(p); }
  function requirePerm(p) { if (!hasPerm(p)) throw new CtoxError('forbidden', `Recht ${p} fehlt`); }
  async function readArtifactContent(hash) {
    const a = await db.get('artifacts', hash);
    if (a) return a.content;
    const c = await db.get('sim_core', `file:${hash}`);
    if (c) return c.content;
    throw new CtoxError('not_found', `Artefakt ${hash} fehlt`);
  }
  // ---- Quelltext-API (§10.2a Nr. 7, §16.22) – Entwürfe je (App, Nutzer) als Dokumente in app_drafts ----
  const PLATFORM_APP = Object.freeze({ id: 'ctox-platform', ver: '1' });
  const MANIFEST_FIELDS_PROTECTED = new Set(['app_id', 'app_ver', 'files', 'generations', 'retires', 'provenance', 'author', 'created_at']);
  const draftId = (appId) => `${actor.id}/${appId}`;
  const MANIFEST_FILE = 'manifest.json';
  function noManifestFile(path, field = 'path') {
    if (path === MANIFEST_FILE) throw new CtoxError('invalid', 'manifest.json ist im Entwurf keine Datei – Felder über draft.setManifest (§16.26)', { path: field });
  }
  // Wirksame Manifest-Verweise des Entwurfs: setManifest-Felder vor Basis-Manifest
  function effectiveRefs(appId, d) {
    const baseMan = d?.base_app_ver ? versionDoc(appId, d.base_app_ver)?.manifest : null;
    const m = d?.manifest ?? {};
    return {
      entry: m.entry ?? baseMan?.entry ?? 'index.js',
      handlers: { ...(m.handlers ?? baseMan?.handlers ?? {}) },
      styles: [...(m.styles ?? baseMan?.styles ?? [])],
    };
  }
  const draftOf = (appId) => visible(mirrorGet('app_drafts', draftId(appId)));
  const activeVerOf = (appId) => visible(mirrorGet('business_apps', appId))?.active_ver ?? null;
  async function readVersionFile(appId, ver, path) {
    const hash = versionDoc(appId, ver)?.manifest?.files?.[path];
    if (!hash) throw new CtoxError('not_found', `${path} nicht in ${appId}@${ver}`);
    return readArtifactContent(hash);
  }
  // Synchroner Lese-Ändere-Schreib-Schritt auf dem eigenen Entwurf (optimistisch im Spiegel, dann Outbox).
  function draftMutate(appId, mutate) {
    guardAlive();
    requirePerm('apps.source.edit');
    const cur = draftOf(appId);
    let d;
    if (cur) d = cloneValue(cur);
    else {
      const base = activeVerOf(appId);
      d = { id: draftId(appId), app_id: appId, user_id: actor.id, base_app_ver: base, files: {}, manifest: {}, created_at: Date.now() };
      // §16.26: manifest.json ist im Entwurf keine Datei – eine Basisdatei dieses Namens wird beim Anlegen verworfen
      if (base && versionDoc(appId, base)?.manifest?.files?.[MANIFEST_FILE]) d.files[MANIFEST_FILE] = null;
    }
    mutate(d);
    d.updated_at = Date.now();
    return localWrite(PLATFORM_APP, 'put', 'app_drafts', d.id, d).local;
  }
  // §16.44-Ergänzung (3): wie draftMutate, liefert aber {local, result}; result = vom Core bestätigte Revision {status, rev}
  // bzw. rejected{code}. Rückgabe ist weiterhin ein Promise (await/.catch wie bisher) mit den Feldern local und result.
  function draftWrite(appId, mutate) {
    let h = null;
    const local = (async () => {
      guardAlive();
      requirePerm('apps.source.edit');
      const cur = draftOf(appId);
      let d;
      if (cur) d = cloneValue(cur);
      else {
        const base = activeVerOf(appId);
        d = { id: draftId(appId), app_id: appId, user_id: actor.id, base_app_ver: base, files: {}, manifest: {}, created_at: Date.now() };
        if (base && versionDoc(appId, base)?.manifest?.files?.[MANIFEST_FILE]) d.files[MANIFEST_FILE] = null;
      }
      mutate(d);
      d.updated_at = Date.now();
      h = localWrite(PLATFORM_APP, 'put', 'app_drafts', d.id, d);
      await h.local;
    })();
    const result = local.then(async () => {
      const r = await h.result;
      return r.status === 'rejected' ? { status: 'rejected', code: r.code ?? r.reason, reason: r.reason } : { status: 'accepted', rev: r.rev };
    }, (e) => ({ status: 'rejected', code: e?.code ?? 'local_error', reason: String(e?.message ?? e) }));
    local.catch(() => {});
    local.local = local;
    local.result = result;
    return local;
  }
  // §16.44-Ergänzung (2): Revision nur für eine bestätigte, nicht gelöschte Zeile ohne ausstehende Änderungen (sonst null)
  function confirmedDraftRev(appId) {
    const r = mirrorGet('app_drafts', draftId(appId));
    if (!r || r.pending || !r.base?.data || r.base.deleted) return null;
    return r.base.rev ?? null;
  }
  function sourceApi(appId) {
    const viewBase = () => { const d = draftOf(appId); return d ? d.base_app_ver : activeVerOf(appId); };
    async function readOverlay(path) {
      const d = draftOf(appId);
      if (d && Object.prototype.hasOwnProperty.call(d.files, path)) {
        if (d.files[path] === null) throw new CtoxError('not_found', `${path} im Entwurf gelöscht`);
        return d.files[path];
      }
      const base = viewBase();
      if (!base) throw new CtoxError('not_found', `${path} nicht vorhanden (neue App ohne Basis)`);
      return readVersionFile(appId, base, path);
    }
    function overlayPaths() {
      const d = draftOf(appId);
      const base = d ? d.base_app_ver : activeVerOf(appId);
      const paths = new Set(Object.keys(base ? versionDoc(appId, base)?.manifest?.files ?? {} : {}));
      for (const [p, t] of Object.entries(d?.files ?? {})) { if (t === null) paths.delete(p); else paths.add(p); }
      return paths;
    }
    return Object.freeze({
      async list(ver) {
        requirePerm('apps.source.view');
        if (ver !== undefined) return Object.keys(versionDoc(appId, ver)?.manifest?.files ?? {}).sort();
        return [...overlayPaths()].sort();
      },
      async read(path, ver) {
        requirePerm('apps.source.view');
        if (ver !== undefined) return readVersionFile(appId, ver, path);
        return readOverlay(path);
      },
      draft: Object.freeze({
        // A4
        async info() {
          requirePerm('apps.source.view');
          const d = draftOf(appId);
          // rev = Revision der app_drafts-Zeile im Core = base_draft_rev für ctox.coding.turn (§16.44)
          return d ? { base_app_ver: d.base_app_ver, files: cloneValue(d.files), manifest: cloneValue(d.manifest), updated_at: d.updated_at, rev: confirmedDraftRev(appId) } : null;
        },
        write(path, text) {
          return draftWrite(appId, (d) => {
            if (typeof path !== 'string' || !path || typeof text !== 'string') throw new CtoxError('invalid', 'path und text (string) nötig', { path: 'path' });
            noManifestFile(path);
            d.files[path] = text;
          });
        },
        // A7
        async remove(path) {
          requirePerm('apps.source.edit');
          noManifestFile(path);
          if (!overlayPaths().has(path)) throw new CtoxError('not_found', `${path} nicht vorhanden`, { path: 'path' });
          await draftMutate(appId, (d) => { d.files[path] = null; });
        },
        async rename(from, to) {
          requirePerm('apps.source.edit');
          if (typeof to !== 'string' || !to) throw new CtoxError('invalid', 'Zielpfad fehlt', { path: 'to' });
          noManifestFile(from, 'from');
          noManifestFile(to, 'to');
          const paths = overlayPaths();
          if (!paths.has(from)) throw new CtoxError('not_found', `${from} nicht vorhanden`, { path: 'from' });
          if (paths.has(to)) throw new CtoxError('conflict', `${to} existiert bereits`, { path: 'to' });
          const text = await readOverlay(from);
          await draftMutate(appId, (d) => {
            d.files[to] = text;
            d.files[from] = null;
            // §16.26: Verweise im Manifest (Einstieg, Handler, Styles) wandern mit
            const refs = effectiveRefs(appId, d);
            if (refs.entry === from) d.manifest.entry = to;
            const handlers = Object.fromEntries(Object.entries(refs.handlers).map(([type, path]) => [type, path === from ? to : path]));
            if (Object.values(refs.handlers).includes(from)) d.manifest.handlers = handlers;
            if (refs.styles.includes(from)) d.manifest.styles = refs.styles.map((x) => (x === from ? to : x));
          });
        },
        // A5
        async setManifest(fields) {
          if (!isPlainObject(fields)) throw new CtoxError('invalid', 'setManifest braucht ein Objekt', { path: '' });
          for (const k of Object.keys(fields)) if (MANIFEST_FIELDS_PROTECTED.has(k)) throw new CtoxError('invalid', `Manifest-Feld ${k} ist nicht änderbar`, { path: k });
          const clean = normalizeJsonPatch(fields);
          await draftMutate(appId, (d) => {
            const { handlers, ...rest } = clean;
            d.manifest = mergePatch(d.manifest ?? {}, rest);
            // handlers gegen die wirksame Karte (Basis + Entwurf) patchen, damit null auch Basis-Handler entfernt
            if (handlers !== undefined) d.manifest.handlers = handlers === null ? {} : mergePatch(effectiveRefs(appId, d).handlers, handlers);
          });
        },
        // A6: vollständiger collections.schema.json-Inhalt; liefert den Diff gegen die Entwurfsbasis
        async setCollections(content) {
          requirePerm('apps.source.edit');
          const parsed = parseCollectionsFile(content);
          const obj = typeof content === 'string' ? JSON.parse(content) : content;
          const text = `${JSON.stringify(obj, null, 2)}\n`;
          await draftMutate(appId, (d) => { d.files[SCHEMA_FILE] = text; });
          const base = draftOf(appId)?.base_app_ver ?? null;
          const baseParsed = await baseSchemaOf(appId, base);
          return { base_app_ver: base, diff: classifyCollections(baseParsed, parsed, { baseVer: base }) };
        },
        async discard() {
          requirePerm('apps.source.edit');
          if (!draftOf(appId)) return;
          await localWrite(PLATFORM_APP, 'delete', 'app_drafts', draftId(appId), null).local;
        },
        async rebase(ver) {
          if (ver !== null && !versionDoc(appId, ver)) throw new CtoxError('not_found', `Version ${appId}@${ver} unbekannt`, { path: 'ver' });
          await draftMutate(appId, (d) => { d.base_app_ver = ver; });
        },
        // A2/A3: base_app_ver = null → neue App; ohne Angabe gilt die Basis des Entwurfs
        publish(opts = {}) {
          let resolveResult;
          const result = new Promise((r) => { resolveResult = r; });
          const local = (async () => {
            guardAlive();
            if (!hasPerm('apps.source.edit')) { resolveResult({ status: 'rejected', reason: 'forbidden' }); return; }
            const d = draftOf(appId);
            if (!d || (!Object.keys(d.files).length && !Object.keys(d.manifest ?? {}).length)) {
              resolveResult({ status: 'rejected', reason: 'invalid', detail: 'leerer Entwurf' });
              return;
            }
            const base = Object.prototype.hasOwnProperty.call(opts, 'base_app_ver') ? opts.base_app_ver : d.base_app_ver;
            const r = await callLeader('publishDraft', [appId, base ?? null, d.files, d.manifest ?? {}]);
            if (r.status !== 'rejected') await localWrite(PLATFORM_APP, 'delete', 'app_drafts', d.id, null).local; // nur der eigene Entwurf
            resolveResult(r);
          })();
          local.catch((e) => resolveResult({ status: 'rejected', reason: e?.code ?? 'local_error', detail: String(e?.message ?? e) }));
          return { local: local.catch(() => {}), result, cancel: () => false };
        },
      }),
    });
  }
  async function baseSchemaOf(appId, base) {
    if (!base) return { collections: {}, migrations: {} };
    const man = versionDoc(appId, base)?.manifest;
    if (man?.files?.[SCHEMA_FILE]) return parseCollectionsFile(await readArtifactContent(man.files[SCHEMA_FILE]));
    return manifestCollectionsToParsed(man?.collections);
  }
  const appsLive = makeLive(['c:business_apps', 'c:business_app_versions', 'activation', 'actor'], () => {
    const out = [];
    for (const r of mirror.get('business_apps')?.values() ?? []) {
      const d = visible(r);
      if (!d) continue;
      if (Array.isArray(d.roles) && !d.roles.includes(actor.role)) continue;
      const a = activations.get(d.id);
      const man = versionDoc(d.id, a?.activated_ver ?? d.active_ver)?.manifest ?? versionDoc(d.id, d.active_ver)?.manifest;
      out.push({
        id: d.id, title: d.title ?? d.id, active_ver: d.active_ver, activated_ver: a?.activated_ver ?? null,
        state: a?.state ?? 'received', target_ver: a?.target_ver ?? null, error: a?.error ?? null,
        layout: { ...normalizeLayout(man?.layout) }, styles: [...(man?.styles ?? [])],
      });
    }
    return out.sort((x, y) => (x.id < y.id ? -1 : 1));
  });
  // window_state (§16.19 Nr. 3): je Tab (tabKey) und Nutzer (DB je Nutzer)
  const windowStateApi = Object.freeze({
    async get(key) {
      guardAlive();
      const r = await db.get('window_state', `tab:${tabKey}:${key}`);
      return r ? r.value : null;
    },
    async put(key, value) {
      guardAlive();
      if (typeof key !== 'string' || !key) throw new CtoxError('invalid', 'key fehlt', { path: 'key' });
      let v;
      try { v = structuredClone(value); } catch { throw new CtoxError('invalid', 'Wert nicht structured-clone-fähig', { path: 'value' }); }
      await db.tx((t) => t.put('window_state', { key: `tab:${tabKey}:${key}`, tab_key: tabKey, value: v, at: Date.now() }));
    },
    async delete(key) { guardAlive(); await db.tx((t) => t.del('window_state', `tab:${tabKey}:${key}`)); },
  });
  const shellApi = Object.freeze({
    apps: appsLive,
    activation: Object.freeze({
      get: (appId) => makeLive(['activation'], () => (activations.has(appId) ? { ...activations.get(appId) } : null)),
      onChange(cb) { activationListeners.add(cb); return () => activationListeners.delete(cb); },
      // §16.19 Nr. 2: Version für diesen Client failed, Zeiger zurück auf die letzte funktionierende, app_activation failed
      reportFailed: (appId, ver, reason) => callLeader('reportFailed', [appId, String(ver), String(reason ?? 'failed')]),
    }),
    metrics: metricsLive,
    leader: leaderLive,
    status: statusStream,
    /** @deprecated seit §16.19 – Alias für `status` (CHANGES.md). */
    statusZone: statusStream,
    windowState: windowStateApi,
    // ---- §16.39 Nr. 1/2 + Ergänzung (LIBS2, 18:40): Artefakt-URLs, Bindung je MOUNT (bind) – gleiche Wurzelform wie der Kernel ----
    // /~a/<app>@<ver>~shell@<sv>~lib:<name>@<M.m>…/<pfad> (Libs nach Name sortiert); Shell: /~a/shell@<sv>~lib:…/…;
    // Bibliotheken: /~a/lib:<name>@<M.m>[~lib:…]/… (immutable). url() ohne Bindung nimmt die aktuell aktive Shell.
    artifacts: (() => {
      const LIB_PREFIX = 'lib:', LIB_NAME = /^[a-z0-9][a-z0-9._-]*$/, SEG = /^[A-Za-z0-9._+-]+$/;
      const encPath = (path) => {
        if (typeof path !== 'string' || !path || path.startsWith('/') || path.split('/').some((x) => x === '..' || x === '.' || x.includes('\\')) || /[?#\x00]/.test(path)) throw new CtoxError('invalid', 'Ungültiger Artefaktpfad', { path: 'path' });
        return path.split('/').map(encodeURIComponent).join('/');
      };
      const activeShellVer = () => activations.get('shell')?.activated_ver ?? visible(mirrorGet('business_apps', 'shell'))?.active_ver ?? null;
      const vdoc = (appId, ver) => visible(mirrorGet('business_app_versions', `${appId}@${ver}`));
      function resolveLibMinor(name, major) {
        const minors = [];
        for (const r of mirror.get('business_app_versions')?.values() ?? []) {
          const d = visible(r); const m = d && d.app_id === `${LIB_PREFIX}${name}` ? /^(\d+)\.(\d+)$/.exec(String(d.app_ver)) : null;
          if (m && m[1] === String(major)) minors.push(Number(m[2]));
        }
        minors.sort((a, b) => a - b);
        return minors.length ? `${major}.${minors.at(-1)}` : null;
      }
      function resolveLibs(appId, ver, shellVer) {
        const bound = { ...(shellVer ? vdoc('shell', shellVer)?.manifest?.libs ?? {} : {}), ...(vdoc(appId, ver)?.manifest?.libs ?? {}) };
        const libs = {};
        for (const [name, majorRaw] of Object.entries(bound)) {
          const r = LIB_NAME.test(name) ? resolveLibMinor(name, String(majorRaw).split('.')[0]) : null;
          if (r) libs[name] = r;
        }
        return libs;
      }
      const libSuffix = (libs, skip = null) => Object.keys(libs).filter((n) => n !== skip).sort().map((n) => `~lib:${n}@${libs[n]}`).join('');
      function check(appId, ver) {
        if (typeof appId !== 'string' || !appId || ver == null || ver === '') throw new CtoxError('invalid', 'appId und ver nötig', { path: 'appId' });
        if (!SEG.test(String(ver))) throw new CtoxError('invalid', 'Version mit unzulässigen Zeichen', { path: 'ver' });
      }
      function makeBinding(appId, ver, o = {}) {
        check(appId, ver); ver = String(ver);
        if (appId.startsWith(LIB_PREFIX)) {
          const name = appId.slice(LIB_PREFIX.length);
          if (!LIB_NAME.test(name) || !/^\d+\.\d+$/.test(ver)) throw new CtoxError('invalid', 'lib-URL braucht lib:<name> und <major>.<minor>', { path: 'ver' });
          const libs = { ...(o.libs ?? {}) };
          return deepFreeze({ appId, ver, shellVer: null, libs, root: `/~a/${LIB_PREFIX}${name}@${ver}${libSuffix(libs, name)}/` });
        }
        const shellVer = appId === 'shell' ? ver : String(o.shellVer ?? activeShellVer() ?? '');
        if (!shellVer) throw new CtoxError('unavailable', 'Keine aktive Shell-Version für die Instanz-URL', { path: 'shellVer' });
        const libs = resolveLibs(appId, ver, shellVer);
        const root = appId === 'shell' ? `/~a/shell@${encodeURIComponent(ver)}${libSuffix(libs)}/` : `/~a/${encodeURIComponent(appId)}@${encodeURIComponent(ver)}~shell@${encodeURIComponent(shellVer)}${libSuffix(libs)}/`;
        return deepFreeze({ appId, ver, shellVer, libs, root });
      }
      return Object.freeze({
        async read(hash) {
          guardAlive();
          const a = await db.get('artifacts', hash);
          if (!a) throw new CtoxError('not_found', `Artefakt ${hash} nicht lokal`);
          return new Blob([a.content], { type: /\.m?js$/.test(a.path ?? '') ? 'text/javascript' : /\.css$/.test(a.path ?? '') ? 'text/css' : 'text/plain' });
        },
        async manifest(appId, ver) { return versionDoc(appId, ver)?.manifest ?? null; },
        /** §16.39 Nr. 1 (A6), synchron: URL mit der aktuell aktiven Shell (oder o.shellVer); keine Merkfunktion je app@ver. */
        url(appId, ver, path, o = {}) { guardAlive(); return makeBinding(appId, ver, o).root + encPath(path); },
        /** §16.39 (Bindung je MOUNT): beim Mount einmal aufrufen; Ergebnis ist eingefroren, url(path) synchron. */
        bind(appId, ver, o = {}) {
          guardAlive(); const b = makeBinding(appId, ver, o);
          // §16.50: Worker (Monaco u. a.) immer gleich-ursprünglich unter der gebundenen Lib-Wurzel, nie data:/blob:
          const workerUrl = (lib, path) => {
            if (typeof lib !== 'string' || !b.libs[lib]) throw new CtoxError('not_found', `Bibliothek ${lib} ist in dieser Bindung nicht gebunden`, { path: 'lib' });
            return `/~a/${LIB_PREFIX}${lib}@${b.libs[lib]}${libSuffix(b.libs, lib)}/${encPath(path)}`;
          };
          return Object.freeze({ ...b, url: (path) => b.root + encPath(path), workerUrl });
        },
      });
    })(),
    source: sourceApi,
    identity: Object.freeze({
      get instance_id() { return instanceId; },
      get user_id() { return userId; },
      /** §16.43: eigene device_pairing_id; Stub deterministisch je (Instanz, Nutzer, Gerät), opts.deviceId geht vor. */
      get device_id() { return opts.deviceId ?? `dev-${fnv1a(`${instanceId}|${userId}|${opts.deviceLabel ?? 'browser'}`)}`; },
      logout: (o) => logout(o),
      // Instanzwechsel: diesen Kernel schließen (Daten bleiben), neuen Kernel für (Instanz, Nutzer) liefern.
      async switchInstance(next = {}) {
        await logout({ deleteData: false });
        return createStubKernel({ ...opts, dbName: undefined, ...next });
      },
    }),
    /** Fensterdaten, die Apps über api.shell setzen (Titel, Header-Element, Größe, Modus, closed). Rendern macht die Shell. */
    window(appId) {
      const tag = `win:${appId}`;
      if (!windowStates.has(appId)) windowStates.set(appId, { title: appId, header: null, size: { width: 0, height: 0 }, mode: 'normal', closed: false });
      const win = windowStates.get(appId);
      return Object.freeze({
        state: makeLive([tag], () => ({ ...win, size: { ...win.size } })),
        setSize(size) { win.size = { width: Number(size?.width ?? 0), height: Number(size?.height ?? 0) }; notify(tag); },
        setMode(mode) { win.mode = String(mode); notify(tag); },
        reopen() { win.closed = false; notify(tag); },
      });
    },
  });

  // ---- Sim-Implementierung (läuft im Leiter) ----
  const simImpl = {
    push: (collection, docs) => corePush(collection, docs),
    setLink(partial = {}) {
      if (partial.state !== undefined && partial.state !== 'online' && partial.state !== 'offline') {
        throw new CtoxError('invalid', 'link.state ∈ online | offline (§16.19 Nr. 7)', { path: 'state' });
      }
      link = { ...link, ...partial };
      netChanged();
      if (link.state === 'online') { track(deliver().then(() => schedulePump())); }
      return { ...link };
    },
    setBacklog(n, collection) {
      if (collection) { const v = Math.max(0, Number(n) || 0); if (v) collBacklog.set(collection, v); else collBacklog.delete(collection); }
      else extraBacklog = Math.max(0, Number(n) || 0);
      netChanged();
    },
    failCollection(collection, code = 'unavailable', message = '') { if (code) collError.set(collection, { code, message: message || code }); else collError.delete(collection); netChanged(); },
    // §16.45 „Neu verbinden“: voller Neuaufbau (simuliert: Verbindung ab, wieder auf, alles neu zustellen)
    async reconnect() {
      guardAlive();
      simRecord.connects++;
      link = { ...link, state: 'offline' }; netChanged();
      link = { ...link, state: 'online' }; netChanged();
      await deliver(); schedulePump();
      return { link: { ...link }, collections: cloneValue(refreshCollections()) };
    },
    rejectNext(reason) { sim0.reject.push(reason); },
    rejectNextUpload(reason) { sim0.rejectUpload.push(reason); },
    migrateNext(o) { sim0.migrate.push(o ?? {}); },
    failCommand(type, error) { sim0.failCmd.set(type, error); },
    registerCommandType(type) { sim0.cmdTypes.add(type); },
    async publishAppVersion(appId, ver, o) {
      await publishInternal(appId, ver, o);
      await waitUntil(() => isTerminalFor(appId, ver) || visible(mirrorGet('business_apps', appId))?.active_ver !== ver || closed);
      return activations.has(appId) ? { ...activations.get(appId) } : null;
    },
    async publishDraft(appId, base, draftFiles, draftManifest) {
      const reject = (reason, detail) => ({ status: 'rejected', reason, ...(detail ? { detail } : {}) });
      const active = activeVerOf(appId);
      if ((active ?? null) !== (base ?? null)) {
        return reject('stale_base', base == null ? `App ${appId} existiert bereits (aktiv: ${active})` : `Basis ${base} ist nicht mehr aktiv (aktiv: ${active ?? '–'})`);
      }
      if (publishing.has(appId)) return reject('busy', `Veröffentlichung von ${appId} läuft`);
      const baseManifest = base ? versionDoc(appId, base)?.manifest : null;
      const files = {};
      for (const [p, h] of Object.entries(baseManifest?.files ?? {})) files[p] = await readArtifactContent(h);
      for (const [p, t] of Object.entries(draftFiles ?? {})) { if (t === null) delete files[p]; else files[p] = t; }
      delete files[MANIFEST_FILE]; // §16.26: keine Datei, nur setManifest-Felder zählen
      if (!Object.keys(files).length) return reject('invalid', 'leerer Entwurf');
      const mf = { ...(draftManifest ?? {}) };
      // §16.26: Verweise aus dem tatsächlichen Entwurfsstand prüfen
      const entry = mf.entry ?? baseManifest?.entry ?? 'index.js';
      const handlers = { ...(mf.handlers ?? baseManifest?.handlers ?? {}) };
      for (const [type, path] of Object.entries(handlers)) if (path === null) delete handlers[type];
      const styles = mf.styles ?? baseManifest?.styles ?? [];
      const dangling = [];
      if (files[entry] === undefined) dangling.push(`entry → ${entry}`);
      for (const [type, path] of Object.entries(handlers)) if (files[path] === undefined) dangling.push(`handlers.${type} → ${path}`);
      for (const path of styles) if (files[path] === undefined) dangling.push(`styles → ${path}`);
      if (dangling.length) return reject('invalid', `Verweis auf fehlende Datei: ${dangling.join(', ')}`);
      let parsedBase, parsedNext;
      try {
        parsedBase = await baseSchemaOf(appId, base);
        parsedNext = files[SCHEMA_FILE] !== undefined ? parseCollectionsFile(files[SCHEMA_FILE]) : parsedBase;
      } catch (e) { return reject('invalid', e.message); }
      const diff = classifyCollections(parsedBase, parsedNext, { baseVer: base });
      const missing = diff.filter((x) => x.needsMigration && (!x.migration || (x.migration.migrate_fn && files[x.migration.migrate_fn] === undefined))).map((x) => x.name);
      if (missing.length) return reject('invalid', `Migration fehlt: ${missing.join(', ')}`);
      const migrate = {};
      for (const x of diff.filter((y) => y.kind === 'generation')) {
        if (Array.isArray(x.migration.steps)) { const steps = x.migration.steps; migrate[x.name] = (doc) => applyMigrationSteps(doc, steps); continue; }
        let fn;
        try { fn = (await importModuleText(files[x.migration.migrate_fn])).default; } catch (e) { return reject('invalid', `${x.migration.migrate_fn}: ${e.message}`); }
        if (typeof fn !== 'function') return reject('invalid', `${x.migration.migrate_fn}: kein default export`);
        migrate[x.name] = fn;
      }
      const head = (await db.get('sim_core', 'core'))?.head ?? 0;
      const ver = `s${head + 1}`;
      // Befund B4: ALLE nicht berechneten Felder des Basis-Manifests erben (triggers, command_rights,
      // foreign_collections, exports, row_filters, write_guard, …); im Entwurf gesetzte Felder gehen vor.
      const computed = new Set([...MANIFEST_FIELDS_PROTECTED, 'title', 'entry', 'handlers', 'capabilities_required', 'min_app_api', 'layout', 'styles', 'collections']);
      const manifestExtra = {};
      for (const [k, v] of Object.entries(baseManifest ?? {})) if (!computed.has(k)) manifestExtra[k] = cloneValue(v);
      for (const [k, v] of Object.entries(mf)) if (!computed.has(k)) manifestExtra[k] = cloneValue(v);
      try {
        await publishInternal(appId, ver, {
          files, entry, title: mf.title,
          collections: parsedToManifestCollections(parsedNext), incompatible: diff.filter((x) => x.kind === 'generation').map((x) => x.name),
          migrate, handlers, styles, capabilitiesRequired: mf.capabilities_required ?? baseManifest?.capabilities_required,
          minAppApi: mf.min_app_api ?? baseManifest?.min_app_api, layout: mf.layout,
          manifestExtra, locallyModified: true, baseAppVer: base ?? null,
        });
      } catch (e) {
        const msg = String(e?.message ?? e);
        return reject(msg.includes('stale_base') ? 'stale_base' : msg.includes('busy') ? 'busy' : (e?.code ?? 'unavailable'), msg);
      }
      return { status: 'accepted', rev: ver, diff: diff.map(({ name, kind }) => ({ name, kind })) };
    },
    async reportFailed(appId, ver, reason) {
      const cur = activations.get(appId) ?? {};
      const good = (cur.good_vers ?? []).filter((v) => v !== ver);
      const lastGood = cur.activated_ver && cur.activated_ver !== ver ? cur.activated_ver : good.at(-1) ?? null;
      const v = await setActivation(appId, { target_ver: ver, activated_ver: lastGood, good_vers: good, state: 'failed', error: `reported: ${reason}` });
      return { ...v };
    },
    corruptNextFetch(n) { corruptNextFetch += n; },
  };
  const forward = (name) => (...args) => callLeader(name, args);

  // ---- Sim-Fassade (Test-Hooks) ----
  const sim = {
    rejectNext: (reason = 'simulated_rejection') => forward('rejectNext')(reason),
    rejectNextUpload: (reason = {code:'forbidden',reason:'Upload denied'}) => forward('rejectNextUpload')(reason),
    migrateNext: (o = {}) => forward('migrateNext')({ dropFields: o.dropFields ?? [] }),
    failCommand: (type, error = 'simulated failure') => forward('failCommand')(type, error),
    /** Registriert zugleich den Typ in der Befehls-Registry des Leiters (K-WJ-B2); unbekannte Typen lehnt der Core beim Schreiben ab. */
    onCommand(type, fn) { sim0.cmdHandlers.set(type, fn); forward('registerCommandType')(type).catch(() => {}); },
    /** CAP-3d: Plattform-Allowlist für process.run (Name → Pfad oder {program|interpreter+script, env}); null = Standard. */
    setProcessAllowlist(map) { processAllowlist = map ? cloneValue(map) : null; },
    push: (collection, docs) => forward('push')(collection, docs),
    setLink: (partial) => forward('setLink')(partial),
    setBacklog: (n, collection) => forward('setBacklog')(n, collection),
    failCollection: (collection, code, message) => forward('failCollection')(collection, code, message),
    get transportConnects() { return simRecord.connects; },
    publishAppVersion(appId, ver, o = {}) {
      if (o.module) return Promise.reject(new CtoxError('invalid', 'In-Memory-Module registriert die Shell bzw. testkit/app-host.mjs, nicht der Kernel (§16.19 Nr. 1)'));
      const { migrate, ...rest } = o;
      if (isLeader || !bc) return simImpl.publishAppVersion(appId, ver, { ...rest, migrate });
      return (async () => {
        await callLeader('publishAppVersion', [appId, ver, rest]); // migrate-Funktionen sind nicht übertragbar
        await waitUntil(() => isTerminalFor(appId, ver) || closed);
        return activations.has(appId) ? { ...activations.get(appId) } : null;
      })();
    },
    /** §16.15a Nr. 3: die nächsten n Artefakt-Abrufe liefern beschädigte Bytes (Kernel holt neu). */
    corruptNextFetch: (n = 1) => forward('corruptNextFetch')(n),
    /** Alte Generationen sofort verwerfen (statt nach generationGraceMs). */
    retireGenerationsNow: async (appId) => {
      const a = activations.get(appId);
      const man = a?.activated_ver ? versionDoc(appId, a.activated_ver)?.manifest : null;
      return retireGenerations(appId, a?.activated_ver, Object.values(man?.retires ?? {}));
    },
    // Loader-Rolle (§16.15): Shell starten und live tauschen.
    async startShell(shellModule, o = {}) {
      if (typeof shellModule?.startShell !== 'function') throw new CtoxError('invalid', 'Shell-Modul exportiert kein startShell()');
      await ready;
      await shellModule.startShell(kernel, { restored: o.restored });
      currentShell = shellModule;
      return true;
    },
    async swapShell(nextShellModule) {
      if (typeof nextShellModule?.startShell !== 'function') throw new CtoxError('invalid', 'Shell-Modul exportiert kein startShell()');
      const prev = currentShell;
      let snap;
      if (prev?.stopShell) {
        try { snap = await prev.stopShell(); } catch (e) {
          zoneReport('shell', { level: 'warn', code: 'shell.snapshot_failed', text: `stopShell() warf: ${e?.message ?? e}` }, 'kernel');
        }
      }
      snap = checkSnapshot('shell', snap);
      await windowStateApi.put('shell', snap ?? null);
      try {
        await withTimeout(Promise.resolve().then(() => nextShellModule.startShell(kernel, { restored: cloneValue(snap) })), shellStartTimeoutMs,
          () => { try { nextShellModule.stopShell?.(); } catch { /* egal */ } });
        currentShell = nextShellModule;
        zoneClear('shell', 'shell.swap_failed');
        return { swapped: true, restored: snap };
      } catch (err) {
        // §10.1: defekte Shell → vorherige Shell-Version starten und Fehler melden
        zoneReport('shell', { level: 'error', code: 'shell.swap_failed', text: `Neue Shell startet nicht (${err?.message ?? err}); vorherige Shell läuft`, ref: null }, 'kernel');
        if (prev) await prev.startShell(kernel, { restored: cloneValue(snap) });
        return { swapped: false, restored: snap, error: String(err?.message ?? err) };
      }
    },
    setPermissions(list) { actor = { ...actor, permissions: [...list] }; notify('actor'); post({ type: 'actor', actor }); },
    setActor(partial) { actor = { ...actor, ...partial }; notify('actor'); post({ type: 'actor', actor }); },
    setLocale(l) { locale = String(l); notify('locale'); },
    setTranslations(l, dict) { translations[l] = { ...(translations[l] ?? {}), ...dict }; notify('locale'); },
    setTheme(tokens) { theme = { ...tokens }; notify('theme'); },
    async evictBlobs() {
      let n = 0;
      await db.tx(async (t) => { for (const b of await t.all('blobs')) if (!b.pinned) { await t.del('blobs', b.ref); n++; } });
      return n;
    },
    async bumpLeaderGen() { await db.tx(async (t) => { const g = ((await t.get('meta', 'leader_gen'))?.value ?? 0) + 1; await t.put('meta', { key: 'leader_gen', value: g }); }); },
    dropBroadcasts(on = true) { dropBroadcasts = !!on; },
    reconcile: () => reconcile(),
    async inspect(collection, id) { return db.get('docs', [collection, id]); },
    outbox: async () => (await db.all('outbox')).sort(byOpId),
    archiveLocal: async () => db.all('archive_local'),
    windowStateRows: async () => db.all('window_state'),
    /** @deprecated seit §16.19 – Alias für `windowStateRows` (CHANGES.md). */
    windowState: async () => db.all('window_state'),
    coreHead: async () => (await db.get('sim_core', 'core'))?.head ?? 0,
    get checkpoint() { return checkpoint; },
    get lastVisible() { return simRecord.lastVisible; },
    get activationReports() { return simRecord.activationReports; },
    get navigations() { return simRecord.navigations; },
    get dialogs() { return simRecord.dialogs; },
    get openedFiles() { return simRecord.openedFiles; },
    get staleDiscards() { return simRecord.staleDiscards; },
    get refetches() { return simRecord.refetches; },
    async settle(timeoutMs = 10000) {
      const t0 = Date.now();
      for (;;) {
        await new Promise((r) => setTimeout(r, Math.max(5, latencyMs / 2)));
        const ob = db ? await db.all('outbox') : [];
        if (!ob.length && !undelivered.length && !inflight.size && !tasks.size && !waitList.length) return true;
        if (Date.now() - t0 > timeoutMs) throw new Error(`settle: nach ${timeoutMs} ms nicht ruhig (outbox ${ob.length}, undelivered ${undelivered.length}, inflight ${inflight.size}, tasks ${tasks.size})`);
      }
    },
  };

  let currentShell = null;
  // ---- Schließen / Abmelden (§16.7) ----
  let ticker = null, reconcileTimer = null;
  const onVis = () => { track(reconcile()); };
  async function close() {
    if (closed) return;
    closed = true;
    for (const t of timers) clearTimeout(t);
    timers.clear();
    clearInterval(ticker);
    clearInterval(reconcileTimer);
    if (typeof document !== 'undefined') document.removeEventListener('visibilitychange', onVis);
    if (typeof window !== 'undefined') window.removeEventListener('focus', onVis);
    lockAbort?.abort();
    const rel = releaseLock; releaseLock = null; rel?.();
    isLeader = false;
    for (const l of [...allLives]) l.dispose();
    allLives.clear();
    activationListeners.clear();
    routeListeners.clear();
    for (const w of waitList.splice(0)) w.resolve();
    for (const w of rpcWaiters.values()) w.rej(new CtoxError('unavailable', 'Kernel geschlossen'));
    rpcWaiters.clear();
    try { bc?.close(); } catch { /* egal */ }
    bc = null;
    db?.close();
  }
  async function logout({ deleteData = false } = {}) {
    loggedOut = true;
    await close();
    mirror.clear();
    zone.length = 0;
    if (deleteData && db) await deleteBackend(dbName, db.kind);
  }

  // ---- Start ----
  const kernel = {
    instanceId, userId, dbName, tabId, ready,
    get isLeader() { return isLeader; },
    get storageKind() { return db?.kind ?? null; },
    appApi: (appId, appVer) => makeAppApi(appId, appVer),
    // Shell-Kernel-Schnittstelle direkt am Kernel (§16.15/§16.19) und als Objekt (kernel.shell / shellApi())
    apps: shellApi.apps,
    activation: shellApi.activation,
    metrics: shellApi.metrics,
    leader: shellApi.leader,
    status: shellApi.status,
    /** @deprecated seit §16.19 – Alias für `status` (CHANGES.md). */
    statusZone: shellApi.status,
    windowState: shellApi.windowState,
    artifacts: shellApi.artifacts,
    source: shellApi.source,
    identity: shellApi.identity,
    shell: shellApi,
    shellApi: () => shellApi,
    /** §16.45: voller Neuaufbau der Verbindung (nie Seiten-Reload); im Leser über den Leiter. */
    reconnect: () => callLeader('reconnect', []),
    sim,
    close,
    logout,
  };
  (async () => {
    db = await openBackend(dbName, opts.storage);
    if (closed) return;
    await loadFromStorage();
    if (hasLocks) {
      bc = new BroadcastChannel(channelName);
      bc.onmessage = (ev) => { onMessage(ev.data).catch((e) => { if (!closed) console.error('[ctox-stub] message', e); }); };
    }
    await elect();
    if (!isLeader) post({ type: 'hello' });
    loaded = true;
    for (const r of preReady.splice(0)) r();
    ticker = setInterval(() => { if (outboxStat.count) notify('metrics'); }, 1000);
    ticker.unref?.();
    reconcileTimer = setInterval(() => { track(reconcile()); }, 2000);
    reconcileTimer.unref?.();
    if (typeof document !== 'undefined') document.addEventListener('visibilitychange', onVis);
    if (typeof window !== 'undefined') window.addEventListener('focus', onVis);
    if (typeof navigator !== 'undefined' && navigator.storage?.persist && typeof window !== 'undefined') navigator.storage.persist().catch(() => {});
    readyResolve();
  })().catch((err) => { console.error('[ctox-stub] Start fehlgeschlagen', err); readyResolve(Promise.reject(err)); });
  return kernel;
}

// Shell-Kernel-Schnittstelle (§10.2a Nr. 8, §16.15) – nur für die Shell, nicht für Apps.
export function createShellKernel(kernel) {
  if (!kernel?.shellApi) throw new CtoxError('invalid', 'createShellKernel braucht einen Kernel aus createStubKernel()');
  return kernel.shellApi();
}
