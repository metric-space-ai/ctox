// kundenpipeline.mail.inbound – Auslöser-Handler (Strang SKL-B4, Kern §16.34 Nr. 3): eingehende Mail → Vorgang.
// Port aus decision_hub.rs project_inbound_messages :60 (Projektion) und enqueue_triage_task :183 (Triage-Auftrag).
// Auslöser: Manifest `triggers:[{handler:'handlers/kundenpipeline.mail.inbound.js', on:{collection:
// 'communication_messages', op:'insert'}}]` mit foreign_collections communication_messages {owner_app:
// 'conversations', ops:['read','watch']}; der Eigentümer exportiert 'watch' (Manifest-Wunsch, V4-HANDLERS.md).
// Aufruf je Änderung: command = {type:'trigger', trigger_id, change:{collection, op, doc}} (§16.20). Rust lief als
// Stapel (200 Mails je Sync-Zyklus) aus der Kanal-DB; hier genau die geänderte Nachricht.
// Triage: Template prompts/triage.md, App-Skill kundenpipeline/triage; das Ergebnis kommt als Rückruf
// kundenpipeline.triage.write (on_complete) bzw. kundenpipeline.triage.failed (on_failed, auch Abbruch).
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import {
  COL_ENTSCHEIDUNGEN, COL_PROJEKTE, COL_VORGAENGE, WRAP_WIDTH, auditEntry, cleanBodyOr, decisionRecord,
  deterministicId, kurz, nonEmpty, routeSender, senderAddressOf, stripMailBallast, upsert, workspaceDecisionUserIds,
  wrapText,
} from './_lib/hub.js';

const str = (v) => (typeof v === 'string' ? v : '');

/** Nachricht aus dem Auslöser (change.doc); direkter Aufruf mit payload.message für Tests/Nachholen. */
function messageOf(command) {
  const doc = command?.change?.doc ?? command?.payload?.message;
  return doc && typeof doc === 'object' ? doc : null;
}

// ref: decision_hub.rs enqueue_triage_task :183-240 – Prompt (prompts/triage.md), Titel, Thread, Idempotenz.
async function enqueueTriage(ctx, vorgangId, vorgang, projekt) {
  const betreff = str(vorgang.quelle_json?.betreff);
  const instruction = await renderPrompt(ctx, 'triage', {
    kunde: projekt.name,
    code: projekt.code_projekt,
    absender: senderAddressOf(vorgang),
    betreff,
    body: kurz(cleanBodyOr(vorgang), 4000),
  });
  return ctx.cap.agent.task.enqueue({
    title: `Triage: ${kurz(betreff, 60)}`,
    instruction: instruction.replace(/\n$/, ''),
    skill: 'kundenpipeline/triage',
    thread_key: `kundenpipeline/triage/${vorgangId}`,
    priority: 'normal',
    payload: { source: 'decision-hub-triage', vorgang_id: vorgangId },
    idempotency_key: `kpl-triage-${vorgangId}`,
    on_complete: 'kundenpipeline.triage.write',
    on_failed: 'kundenpipeline.triage.failed',
  });
}

export default async function handle(command, ctx) {
  const message = messageOf(command);
  if (!message) return { ok: true, skipped: 'no_message' };
  // Rust-Filter: WHERE direction = 'inbound' AND channel = 'email'
  if (message.direction !== 'inbound' || message.channel !== 'email') return { ok: true, skipped: 'not_inbound_email' };
  const messageKey = str(message.message_key) || str(message.id);
  if (!messageKey) return { ok: true, skipped: 'no_message_key' };
  const vorgangId = deterministicId('kpl-v', messageKey);
  // Rust existing_record_ids: vorhandener Vorgang (auch gelöscht) → nichts tun. Macht den Auslöser idempotent.
  if (await ctx.get(COL_VORGAENGE, vorgangId)) return { ok: true, skipped: 'exists', vorgang_id: vorgangId };

  const senderAddress = str(message.sender_address);
  const subject = str(message.subject);
  const projekte = await ctx.read(COL_PROJEKTE, { selector: {} });
  const { treffer, vorschlag } = routeSender(projekte.filter((p) => p?.is_deleted !== true), senderAddress);
  const now = ctx.now();
  // ref: :120-149. Abweichung (Befund B1): triage_json/run_json fehlen statt null (Schema erlaubt nur object).
  const vorgang = {
    id: vorgangId,
    title: kurz(nonEmpty(subject, '(kein Betreff)'), 120),
    status: treffer ? 'zugeordnet' : 'eingegangen',
    kunde_id: treffer?.id ?? '',
    kunde_name: treffer?.name ?? '',
    quelle_json: {
      kanal: 'mail',
      message_ref: messageKey,
      thread_ref: str(message.thread_key),
      absender: senderAddress,
      absender_name: str(message.sender_display),
      betreff: subject,
      body_clean: stripMailBallast(str(message.body_text)),
      eingegangen: str(message.external_created_at),
    },
    mails_json: [],
    audit_json: [auditEntry(now, 'ingest', 'system', 'system')],
    notes: '',
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  };

  if (treffer) {
    // Einreihen vor dem Schreiben: scheitert es, bleibt kein Vorgang ohne Triage zurück (der nächste Auslöser
    // versucht es erneut, idempotency_key verhindert einen zweiten Auftrag).
    const task = await enqueueTriage(ctx, vorgangId, vorgang, treffer);
    vorgang.triage_task_id = task?.task_id ?? null;
    await upsert(ctx, COL_VORGAENGE, vorgang);
    return { ok: true, vorgang_id: vorgangId, status: vorgang.status, task_id: vorgang.triage_task_id };
  }
  await upsert(ctx, COL_VORGAENGE, vorgang);
  // ref: :155-175 – keine Zuordnung: Entscheidungskarte „zuordnung“ mit Mail und Routing-Vorschlag.
  const zeilen = ['▸ MAIL', ...wrapText(cleanBodyOr(vorgang), WRAP_WIDTH)];
  if (vorschlag) zeilen.push('', ...wrapText(`Routing-Vorschlag: ${vorschlag.name}`, WRAP_WIDTH));
  const decisionId = deterministicId('kpl-e-zuord', vorgangId);
  const decision = decisionRecord(decisionId, vorgangId, 'zuordnung', kurz(senderAddressOf(vorgang), 40), zeilen, await workspaceDecisionUserIds(ctx), now);
  await upsert(ctx, COL_ENTSCHEIDUNGEN, decision);
  return { ok: true, vorgang_id: vorgangId, status: vorgang.status, decision_id: decisionId };
}
