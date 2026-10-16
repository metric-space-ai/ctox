// ctox.crew.memory.update – Port von crew_commands.rs:136-193 (4794af03d).
// Das Gedächtnis eines Wesens ist seine Continuity (anchors = gesichertes Wissen,
// narrative = Erfahrung) – jetzt die App-Collections ctox_crew_continuity +
// ctox_crew_continuity_commits (_lib/continuity.js, Port der LCM-Operationen).
// Commit, Kopf, Mitglieds-Touch und die Gedächtnissicht am Mitglied schreibt eine
// einzige ctx.transaction.
import { charCount, fail, invalid } from './_lib/errors.js';
import { CREW_MEMBERS, isoMillis, listMembers, requireText, runCrewControl } from './_lib/crew.js';
import { applyContinuityOp, initContinuityDocuments, memoryView } from './_lib/continuity.js';

const MEMORY_LIMIT = 8000;

export default async function handle(command, ctx) {
  return runCrewControl(command, ctx, async (p, now) => {
    const id = requireText(p, 'member_id');
    // ref: crew_commands.rs:145-150 – Mitglied muss existieren (auch archiviert).
    if (!(await listMembers(ctx)).some((m) => m.id === id)) throw fail('not_found', 'member not found');
    const kind = requireText(p, 'kind');
    if (kind !== 'anchors' && kind !== 'narrative') throw invalid('kind must be anchors or narrative');
    // ref: crew_commands.rs:161-171 – nicht requireText: dessen 200-Zeichen-Grenze machte die 8 000 unerreichbar.
    const bounded = (key) => {
      const value = p[key];
      if (typeof value !== 'string' || value.trim() === '') throw invalid(`${key} is required`);
      if (charCount(value) > MEMORY_LIMIT) throw invalid(`${key} is too long`);
      return value;
    };
    const mode = requireText(p, 'mode');
    let request;
    if (mode === 'diff') request = { mode, diff: bounded('diff') };
    else if (mode === 'replace') {
      request = { mode, find: bounded('find'), replace: typeof p.replace === 'string' ? p.replace : '' };
    } else if (mode === 'full') request = { mode, body: bounded('body') };
    else throw invalid('mode must be diff, replace or full');
    const nowMs = isoMillis(now);
    const document = await ctx.transaction(async (tx) => {
      // ref: crew_commands.rs:158-160 – continuity_init_documents vor der Operation.
      const docs = await initContinuityDocuments(tx, id, nowMs);
      const head = await applyContinuityOp(tx, id, kind, request, nowMs);
      docs[kind] = head;
      // ref: crew_commands.rs:187-191 – Touch des Mitglieds; dazu die Gedächtnissicht,
      // die bisher die Projektion (harness_cockpit_projections.rs:1588-1608) schrieb.
      await tx.patch(CREW_MEMBERS, id, {
        memory: memoryView(docs.anchors, docs.narrative),
        updated_at: now,
        updated_at_ms: nowMs,
      });
      return head;
    });
    return { member_id: id, kind, head_commit_id: document.head_commit_id };
  });
}
