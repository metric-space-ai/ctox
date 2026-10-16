// kundenpipeline.delegate – Port aus decision_hub.rs handle_delegate :899.
// GEMISCHT. Fachlogik hier: Code-Projekt ueber kunde_id -> kundenpipeline_projekte,
// Aufgabentext (payload.beschreibung > triage.aufgabe.beschreibung), Auftrags-Prompt,
// Status inArbeit + run_json + Audit. Faehigkeit: CTOX-Queue-Task -> ctx.cap.agent.task.enqueue
// (Rueckrufe kundenpipeline.auftrag.complete / kundenpipeline.auftrag.failed, Strang SKL-B4).

import { renderPrompt } from 'ctox:shared/prompt-template.js';
import { COL_PROJEKTE, COL_VORGAENGE, actorId, fail, kurz, loadAnyRecord, pushAudit, upsert } from './_lib/hub.js';

const str = (v) => (typeof v === 'string' ? v : undefined);

export async function delegateVorgang(ctx, command, payload) {
  if (typeof payload.vorgang_id !== 'string') fail('invalid', 'vorgang_id is required');
  const vorgangId = payload.vorgang_id;
  const vorgang = await loadAnyRecord(ctx, COL_VORGAENGE, vorgangId);
  if (!vorgang) fail('not_found', `unknown Vorgang \`${vorgangId}\``);
  let codeProjekt = null;
  const kundeId = str(vorgang.kunde_id);
  if (kundeId) {
    // Rust: `.ok().flatten()` – Lesefehler werden verschluckt.
    const projekt = await loadAnyRecord(ctx, COL_PROJEKTE, kundeId).catch(() => null);
    const path = str(projekt?.code_projekt);
    if (path !== undefined && path.trim()) codeProjekt = path;
  }
  const beschreibung = str(payload.beschreibung) ?? str(vorgang.triage_json?.aufgabe?.beschreibung);
  if (beschreibung === undefined) fail('invalid', 'no task description (payload.beschreibung / triage.aufgabe)');
  const titel = str(vorgang.title) ?? 'Kundenauftrag';
  const kunde = str(vorgang.kunde_name) ?? '';
  // ref: decision_hub.rs handle_delegate :899 – Auftragstext in prompts/delegate.md. Der frühere CLI-Rückschrieb
  // (`ctox business-os commands dispatch … kundenpipeline.triage.write` mit leerem triage:{}, der triage_json
  // überschrieb) ist gestrichen: das Ergebnis kommt als Rückruf kundenpipeline.auftrag.complete/.failed.
  const instruction = await renderPrompt(ctx, 'delegate', { kunde, vorgang_id: vorgangId, beschreibung });
  // §16.30: agent.task.enqueue flach. Code-Projekt (workspace_root) und Herkunft reisen im payload, die
  // Idempotenz je Vorgang ist ein eigenes Feld.
  const task = await ctx.cap.agent.task.enqueue({
    title: `Kundenauftrag: ${kurz(titel, 60)}`,
    instruction: instruction.replace(/\n$/, ''),
    thread_key: `kundenpipeline/auftrag/${vorgangId}`,
    priority: 'normal',
    payload: {
      source: 'decision-hub-delegate',
      vorgang_id: vorgangId,
      workspace_root: codeProjekt,
    },
    idempotency_key: `kpl-auftrag-${vorgangId}`,
    on_complete: 'kundenpipeline.auftrag.complete',
    on_failed: 'kundenpipeline.auftrag.failed',
  });
  const taskKey = task?.task_id ?? task?.message_key ?? null;
  const threadKey = task?.thread_key ?? `kundenpipeline/auftrag/${vorgangId}`;
  const now = ctx.now();
  const actor = actorId(command, 'owner');
  vorgang.status = 'inArbeit';
  vorgang.run_json = {
    task_key: taskKey,
    thread_key: threadKey,
    workspace_root: codeProjekt,
    gestartet_ms: now,
  };
  vorgang.updated_at_ms = now;
  pushAudit(vorgang, now, 'auftrag:delegiert', actor, 'system');
  await upsert(ctx, COL_VORGAENGE, vorgang);
  return { ok: true, task_key: taskKey, thread_key: threadKey };
}

export default async function handle(command, ctx) {
  return delegateVorgang(ctx, command, command.payload ?? {});
}
