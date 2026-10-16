// Gemeinsamer Arm ctox.crew.learning.{confirm,update,delete}
// (crew_commands.rs:200-241). Lernsätze sind die App-Collection
// ctox_crew_learnings (früher Kern-Tabelle crew_member_learnings).
import { fail, invalid } from './errors.js';
import { CREW_LEARNINGS, isoMillis, normalized, proseLine, requireText, runCrewControl, safeProse } from './crew.js';

export async function runLearningControl(command, ctx, op) {
  return runCrewControl(command, ctx, async (p, now) => {
    const id = requireText(p, 'learning_id');
    await ctx.transaction(async (tx) => {
      // ref: crew_commands.rs:204-211 – SELECT member_id … ; ensure!(member.is_some())
      const learning = await tx.get(CREW_LEARNINGS, id);
      if (!learning || !learning.member_id) throw fail('not_found', 'learning not found');
      const nowMs = isoMillis(now);
      if (op === 'confirm') {
        // ref: crew_commands.rs:213-217
        await tx.patch(CREW_LEARNINGS, id, { confirmed_by_owner: true, updated_at_ms: nowMs });
      } else if (op === 'delete') {
        // ref: crew_commands.rs:219-221
        await tx.remove(CREW_LEARNINGS, id);
      } else {
        const patch = {};
        if (p.text !== undefined) {
          // ref: crew_commands.rs:223-228
          if (typeof p.text !== 'string') throw invalid('text must be a string');
          const value = proseLine(p.text);
          if (!safeProse(value, 400)) throw invalid('invalid learning text');
          const norm = normalized(value);
          // ref: src/core/crew/mod.rs:114-121 – UNIQUE(member_id, normalized_text);
          // Rust scheiterte am Constraint, hier ausdrücklich als conflict.
          const twins = await tx.read(CREW_LEARNINGS, {
            selector: { member_id: learning.member_id, normalized_text: norm },
          });
          if (twins.some((t) => t.id !== id)) throw fail('conflict', 'learning text already exists for this member');
          patch.text = value;
          patch.normalized_text = norm;
        }
        if (p.archived !== undefined) {
          // ref: crew_commands.rs:229-237
          if (typeof p.archived !== 'boolean') throw invalid('archived must be boolean');
          patch.archived = p.archived;
        }
        // Rust: zwei getrennte UPDATEs in einer Transaktion; leeres Update ist ein No-op.
        if (Object.keys(patch).length > 0) await tx.patch(CREW_LEARNINGS, id, { ...patch, updated_at_ms: nowMs });
      }
    });
    return { learning_id: id };
  });
}
