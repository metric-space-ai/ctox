// Strang SKL-O: Automatisierungsknöpfe der Outbound-App gegen die agent-task-Simulation (Kern §16.30/§16.34).
// Campaign-Setup: Knopf → outbound.campaign.setup.request → enqueue (App-Skill, Template) → Rückruf .done → apply_setup
// bzw. .failed. Anschreiben: Knopf → outbound.pipeline.outreach_draft → enqueue → .done → write_outreach_draft bzw.
// .failed. Recherche: enqueue mit App-Skill outbound/prospect-research. Testnamen beginnen mit der Invariante (SKLO1…),
// tests/mutation-proof.mjs bricht je Invariante eine Zeile und erwartet rot.
import assert from 'node:assert/strict';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { cmd, createTestCtx } from './_ctx.mjs';
import './ctox-shared-loader.mjs'; // ctox:shared/ wie der Service Worker (Handler mit prompt-template.js)
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';

const APP_DIR = fileURLToPath(new URL('../..', import.meta.url));
const { default: setupRequest } = await import('../outbound.campaign.setup.request.js');
const { default: setupDone } = await import('../outbound.campaign.setup.done.js');
const { default: setupFailed } = await import('../outbound.campaign.setup.failed.js');
const { default: draftRequest } = await import('../outbound.pipeline.outreach_draft.js');
const { default: draftDone } = await import('../outbound.pipeline.outreach_draft.done.js');
const { default: draftFailed } = await import('../outbound.pipeline.outreach_draft.failed.js');
const { default: companyResearch } = await import('../outbound.company.research.js');

const NOW = 1_760_000_000_000;
const CAMPAIGN = {
  id: 'camp_1',
  name: 'Nord-Handwerk',
  status: 'setup_required',
  objective: 'Handwerk',
  payload: { briefing: 'Ich möchte 100 Handwerksbetriebe in Norddeutschland per E-Mail anschreiben.', briefing_language: 'de', research_settings: { keep: true } },
  created_at_ms: 1,
  updated_at_ms: 1,
};
const ITEM = {
  id: 'pipe_1',
  campaign_id: 'camp_1',
  company_id: 'co_1',
  contacts: [{ name: 'Erika Muster', email: 'erika@acme.test' }, { name: 'Max Beispiel' }],
  updated_at_ms: 1,
};
const DRAFTING_REQUEST = {
  Produkt_und_Dienstleistungsbeschreibung: 'Auftragsplanung für Handwerk',
  CTA: '15 Minuten Call',
  Signatur: 'Team CTOX',
  company: { name: 'Acme GmbH', website_url: 'https://acme.test', homepage_summary: 'Unternehmen: Acme GmbH' },
  person: { first_name: 'Erika', last_name: 'Muster' },
};
const SETUP_REPLY = {
  status: 'active',
  campaign_payload_patch: {
    campaign_setup: { status: 'configured', summary: 'E-Mail an Handwerk Nord', assumptions: [], blockers: [] },
    active_outreach: { approval_required: true },
    app_extension_requests: [],
    secret_field: 'darf nicht ankommen',
  },
};
const DRAFT = {
  message_mail_subject: 'Auftragsplanung bei Acme',
  message_mail_body: 'Sehr geehrte Frau Muster, …\n\nTeam CTOX',
  message_followup_1: 'Kurze Erinnerung …',
  message_followup_2: 'Letzte Nachricht …',
};

function world(data = {}) {
  const t = createTestCtx({
    now: NOW,
    data: {
      outbound_campaigns: { camp_1: structuredClone(CAMPAIGN) },
      outbound_pipeline_items: { pipe_1: structuredClone(ITEM) },
      ...data,
    },
  });
  t.ctx.cap = {}; // Objekt statt Proxy, damit die Simulation agent.task anhängt
  const sim = createAgentTaskSim({ appId: 'outbound', appDir: APP_DIR, ctxFactory: () => t.ctx });
  sim.attach(t.ctx, null);
  return { t, sim };
}
const setupCmd = (extra = {}) => cmd('outbound.campaign.setup.request', {
  campaign_id: 'camp_1',
  source_command_id: 'cmd_setup_1',
  briefing: CAMPAIGN.payload.briefing,
  template: { id: 'de-mail-handwerk-nord', title: 'Handwerk in Norddeutschland per E-Mail' },
  ...extra,
}, { id: 'cmd_req_1' });
const draftCmd = (extra = {}, id = 'cmd_outreach_1') => cmd('outbound.pipeline.outreach_draft', {
  pipeline_id: 'pipe_1', contact_index: 0, company_id: 'co_1', drafting_request: DRAFTING_REQUEST, ...extra,
}, { id });

test('SKLO1 Campaign-Setup: Befehl → Auftrag mit App-Skill, Template, Rückrufen; Zustand queued; Doppelauslösung ohne Wirkung', async () => {
  const { t, sim } = world();
  const out = await setupRequest(setupCmd(), t.ctx);
  assert.equal(out.applied, true);
  assert.equal(sim.enqueued.length, 1);
  const [{ args, skill, task_id: taskId }] = sim.enqueued;
  assert.deepEqual(skill, { skill: 'outbound/campaign-setup', scope: 'app', path: 'skills/campaign-setup/SKILL.md' });
  assert.equal(args.on_complete, 'outbound.campaign.setup.done');
  assert.equal(args.on_failed, 'outbound.campaign.setup.failed');
  assert.equal(args.idempotency_key, 'outbound-campaign-setup-cmd_setup_1');
  assert.equal(args.thread_key, 'business-os/outbound/camp_1');
  assert.equal(args.title, 'Outbound Campaign einrichten · Nord-Handwerk');
  assert.equal(args.payload.campaign_id, 'camp_1');
  assert.equal(args.payload.source_command_id, 'cmd_setup_1');
  assert.deepEqual(args.payload.selected_template, { id: 'de-mail-handwerk-nord', title: 'Handwerk in Norddeutschland per E-Mail' });
  assert.equal(args.instruction.split('\n')[0], 'Nutze den CTOX Skill outbound/campaign-setup.');
  assert.match(args.instruction, /- campaign_id: camp_1\n- name: Nord-Handwerk\n- command_id_for_writeback: cmd_setup_1\n- selected_template_id: de-mail-handwerk-nord\n- selected_template_title: Handwerk in Norddeutschland per E-Mail\n/);
  assert.match(args.instruction, /User-Briefing:\nIch möchte 100 Handwerksbetriebe in Norddeutschland per E-Mail anschreiben\.\n/);
  assert.doesNotMatch(args.instruction, /ctox business-os|commands dispatch|\{\{/);
  const setup = t.doc('outbound_campaigns', 'camp_1').payload.campaign_setup_task;
  assert.equal(setup.task_id, taskId);
  assert.equal(setup.status, 'queued');
  assert.equal(setup.source_command_id, 'cmd_setup_1');
  assert.equal(setup.skill, 'outbound/campaign-setup');
  const writes = t.calls.writes.length;
  const again = await setupRequest(setupCmd(), t.ctx);
  assert.equal(again.applied, false);
  assert.equal(sim.enqueued.length, 1, 'kein zweiter Auftrag');
  assert.equal(t.calls.writes.length, writes);
  // Freitext ohne Vorlage → custom
  const c = world();
  await setupRequest(setupCmd({ template: undefined, source_command_id: 'cmd_setup_2' }), c.t.ctx);
  assert.match(c.sim.enqueued[0].args.instruction, /- selected_template_id: custom\n- selected_template_title: custom\n/);
});

test('SKLO2 Setup complete → apply_setup mit Patch nur aus dem Schreibvertrag, Status, task_id; Doppelzustellung ohne Wirkung', async () => {
  const { t, sim } = world();
  await setupRequest(setupCmd(), t.ctx);
  const [{ task_id: taskId }] = sim.enqueued;
  const d = await sim.complete(taskId, { text: `Hier das Setup:\n${JSON.stringify(SETUP_REPLY)}` });
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.command.type, 'outbound.campaign.setup.done');
  assert.equal(d.result.applied, true);
  const campaign = t.doc('outbound_campaigns', 'camp_1');
  assert.equal(campaign.status, 'active');
  assert.deepEqual(campaign.payload.campaign_setup, SETUP_REPLY.campaign_payload_patch.campaign_setup);
  assert.deepEqual(campaign.payload.active_outreach, { approval_required: true });
  assert.equal('secret_field' in campaign.payload, false, 'nur erlaubte Schlüssel');
  assert.deepEqual(campaign.payload.research_settings, { keep: true }, 'bestehende Settings bleiben');
  assert.equal(campaign.payload.campaign_setup_task.status, 'completed');
  assert.equal(campaign.payload.campaign_setup_task.task_id, taskId);
  assert.equal(campaign.payload.campaign_setup_task.source_command_id, 'cmd_setup_1');
  assert.equal(campaign.payload.campaign_setup_task.skill, 'outbound/campaign-setup');
  const writes = t.calls.writes.length;
  assert.deepEqual(await sim.complete(taskId, { text: JSON.stringify(SETUP_REPLY) }), { delivered: false, reason: 'duplicate' });
  const again = await setupDone(d.command, t.ctx);
  assert.equal(again.applied, false);
  assert.equal(t.calls.writes.length, writes, 'keine Schreibvorgänge bei Doppelzustellung');
});

test('SKLO3 Setup-Antwort ohne campaign_setup.status → Setup failed mit Grund, kein Patch', async () => {
  const { t, sim } = world();
  await setupRequest(setupCmd(), t.ctx);
  const [{ task_id: taskId }] = sim.enqueued;
  const d = await sim.complete(taskId, { text: '{"campaign_payload_patch":{"research_settings":{"x":1}}}' });
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.result.applied, true);
  const campaign = t.doc('outbound_campaigns', 'camp_1');
  assert.equal(campaign.payload.campaign_setup_task.status, 'failed');
  assert.match(campaign.payload.campaign_setup_task.error, /campaign_setup\.status/);
  assert.deepEqual(campaign.payload.research_settings, { keep: true });
  assert.equal(campaign.status, 'setup_required');
});

test('SKLO4 Setup fail/cancel → Ausgang am Datensatz; zweite Zustellung und veralteter Auftrag ohne Wirkung', async () => {
  const { t, sim } = world();
  await setupRequest(setupCmd(), t.ctx);
  const [{ task_id: taskId }] = sim.enqueued;
  const d = await sim.fail(taskId, { code: 'failed', message: 'Modell nicht erreichbar' });
  assert.equal(d.command.type, 'outbound.campaign.setup.failed');
  assert.equal(d.result.applied, true);
  let setup = t.doc('outbound_campaigns', 'camp_1').payload.campaign_setup_task;
  assert.equal(setup.status, 'failed');
  assert.equal(setup.error, 'Modell nicht erreichbar');
  assert.equal(setup.finished_at_ms, NOW);
  const writes = t.calls.writes.length;
  assert.equal((await setupFailed(d.command, t.ctx)).applied, false);
  assert.equal(t.calls.writes.length, writes);

  const c = world();
  await setupRequest(setupCmd(), c.t.ctx);
  const first = c.sim.enqueued[0].task_id;
  const cancelled = await setupFailed(cmd('outbound.campaign.setup.failed', { task_id: first, status: 'cancelled', result: null }), c.t.ctx);
  assert.equal(cancelled.status, 'cancelled');
  assert.equal(c.t.doc('outbound_campaigns', 'camp_1').payload.campaign_setup_task.status, 'cancelled');

  // Neueres Setup überholt einen alten Auftrag: dessen Rückruf bleibt ohne Wirkung.
  const n = world();
  await setupRequest(setupCmd(), n.t.ctx);
  await setupRequest(setupCmd({ source_command_id: 'cmd_setup_2' }), n.t.ctx);
  const [old, fresh] = n.sim.enqueued.map((e) => e.task_id);
  assert.notEqual(old, fresh);
  const stale = await n.sim.complete(old, { text: JSON.stringify(SETUP_REPLY) });
  assert.equal(stale.result.applied, false);
  assert.equal(n.t.doc('outbound_campaigns', 'camp_1').payload.campaign_setup_task.task_id, fresh);
  assert.equal(n.t.doc('outbound_campaigns', 'camp_1').payload.campaign_setup_task.status, 'queued');
});

test('SKLO5 Anschreiben: Befehl → Auftrag mit App-Skill, Template, Skillbook aus der Collection; Kontakt queued; idempotent', async () => {
  const skillbook = { id: 'business-os.outbound.message_drafting.v1', mission: 'Eigene Mission der Instanz', non_negotiable_rules: ['Nie senden'] };
  const { t, sim } = world({ outbound_skillbooks: { [skillbook.id]: skillbook } });
  const out = await draftRequest(draftCmd(), t.ctx);
  assert.equal(out.applied, true);
  const [{ args, skill, task_id: taskId }] = sim.enqueued;
  assert.deepEqual(skill, { skill: 'outbound/message-drafting', scope: 'app', path: 'skills/message-drafting/SKILL.md' });
  assert.equal(args.on_complete, 'outbound.pipeline.outreach_draft.done');
  assert.equal(args.on_failed, 'outbound.pipeline.outreach_draft.failed');
  assert.equal(args.idempotency_key, 'outbound-outreach-draft-cmd_outreach_1');
  assert.equal(args.title, 'Anschreiben generieren: Acme GmbH');
  assert.deepEqual(args.payload, { pipeline_id: 'pipe_1', contact_index: 0, company_id: 'co_1', request_command_id: 'cmd_outreach_1' });
  assert.match(args.instruction, /^Erstelle ein personalisiertes Outbound-Anschreiben \(Betreff, Mailtext und zwei Follow-ups\)/);
  assert.match(args.instruction, /\npipeline_id: pipe_1\ncontact_index: 0\n/);
  assert.ok(args.instruction.includes(JSON.stringify(DRAFTING_REQUEST, null, 2)), 'drafting_request als JSON');
  assert.match(args.instruction, /Eigene Mission der Instanz/, 'Skillbook aus outbound_skillbooks');
  assert.doesNotMatch(args.instruction, /ctox business-os|commands dispatch|business_os\.get_record|\{\{/);
  const contact = t.doc('outbound_pipeline_items', 'pipe_1').contacts[0];
  assert.equal(contact.outreach_generating, true);
  assert.equal(contact.outreach_status, 'queued');
  assert.equal(contact.outreach_task_id, taskId);
  const writes = t.calls.writes.length;
  assert.equal((await draftRequest(draftCmd(), t.ctx)).applied, false);
  assert.equal(sim.enqueued.length, 1);
  assert.equal(t.calls.writes.length, writes);
  // Ohne gepflegtes Skillbook: Vorgabe aus skillbook-defaults.js
  const d = world();
  await draftRequest(draftCmd(), d.t.ctx);
  assert.match(d.sim.enqueued[0].args.instruction, /Personalisierte Erst- und Follow-up-Anschreiben/);
});

test('SKLO6 Anschreiben complete → write_outreach_draft (4 Felder, drafted); Doppelzustellung ohne Wirkung; unvollständige Antwort → draft_failed', async () => {
  const { t, sim } = world();
  await draftRequest(draftCmd(), t.ctx);
  const [{ task_id: taskId }] = sim.enqueued;
  const d = await sim.complete(taskId, { text: JSON.stringify({ messages: DRAFT }) });
  assert.equal(d.error, undefined, d.error?.message);
  assert.equal(d.command.type, 'outbound.pipeline.outreach_draft.done');
  let contact = t.doc('outbound_pipeline_items', 'pipe_1').contacts[0];
  assert.deepEqual(contact.messages, DRAFT);
  assert.equal(contact.outreach_generating, false);
  assert.equal(contact.outreach_status, 'drafted');
  const writes = t.calls.writes.length;
  assert.deepEqual(await sim.complete(taskId, { text: JSON.stringify({ messages: DRAFT }) }), { delivered: false, reason: 'duplicate' });
  assert.equal((await draftDone(d.command, t.ctx)).applied, false);
  assert.equal(t.calls.writes.length, writes);

  const b = world();
  await draftRequest(draftCmd({ contact_index: 1 }, 'cmd_outreach_2'), b.t.ctx);
  const r = await b.sim.complete(b.sim.enqueued[0].task_id, { text: JSON.stringify({ messages: { ...DRAFT, message_followup_2: '' } }) });
  assert.equal(r.error, undefined, r.error?.message);
  contact = b.t.doc('outbound_pipeline_items', 'pipe_1').contacts[1];
  assert.equal(contact.outreach_generating, false);
  assert.equal(contact.outreach_status, 'draft_failed');
  assert.match(contact.outreach_error, /message_followup_2/);
  assert.equal(contact.messages, undefined, 'kein Teilentwurf');
});

test('SKLO7 Anschreiben fail/cancel → Spinner gelöst, draft_failed/draft_cancelled; zweite Zustellung und fremder Auftrag ohne Wirkung', async () => {
  const { t, sim } = world();
  await draftRequest(draftCmd(), t.ctx);
  const [{ task_id: taskId }] = sim.enqueued;
  const d = await sim.fail(taskId, 'Kontingent erschöpft');
  assert.equal(d.command.type, 'outbound.pipeline.outreach_draft.failed');
  let contact = t.doc('outbound_pipeline_items', 'pipe_1').contacts[0];
  assert.equal(contact.outreach_generating, false);
  assert.equal(contact.outreach_status, 'draft_failed');
  assert.equal(contact.outreach_error, 'Kontingent erschöpft');
  const writes = t.calls.writes.length;
  assert.equal((await draftFailed(d.command, t.ctx)).applied, false);
  assert.equal(t.calls.writes.length, writes);

  const c = world();
  await draftRequest(draftCmd(), c.t.ctx);
  const out = await draftFailed(cmd('outbound.pipeline.outreach_draft.failed', { task_id: c.sim.enqueued[0].task_id, status: 'cancelled', result: null }), c.t.ctx);
  assert.equal(out.applied, true);
  contact = c.t.doc('outbound_pipeline_items', 'pipe_1').contacts[0];
  assert.equal(contact.outreach_status, 'draft_cancelled');
  assert.equal(contact.outreach_generating, false);

  // Neuer Auftrag am selben Kontakt: Rückruf des alten bleibt ohne Wirkung.
  const n = world();
  await draftRequest(draftCmd(), n.t.ctx);
  await draftRequest(draftCmd({}, 'cmd_outreach_9'), n.t.ctx);
  const [old] = n.sim.enqueued;
  const stale = await n.sim.fail(old.task_id, 'alt');
  assert.equal(stale.result.applied, false);
  assert.equal(n.t.doc('outbound_pipeline_items', 'pipe_1').contacts[0].outreach_generating, true);
});

test('SKLO8 Recherche: Auftrag trägt den App-Skill outbound/prospect-research (aufgelöst gegen das Paket)', async () => {
  const { t, sim } = world();
  t.ctx.cap.knowledge = { query: async () => ({ ok: true, rows: [] }) };
  const out = await companyResearch(cmd('outbound.company.research', {
    company: { id: 'co_1', name: 'Acme GmbH', country: 'DE' }, campaign: { id: 'camp_1', name: 'K' },
  }, { id: 'cmd_research_1' }), t.ctx).catch((error) => ({ error }));
  assert.equal(out.error, undefined, out.error?.message);
  assert.equal(sim.enqueued.length, 1);
  assert.deepEqual(sim.enqueued[0].skill, { skill: 'outbound/prospect-research', scope: 'app', path: 'skills/prospect-research/SKILL.md' });
  assert.equal(sim.enqueued[0].args.on_complete, 'outbound.company.research.done');
});
