// ref: store_outbound_commands.rs:4663 outbound_handle_skillbook_seed_defaults (idempotent)
import { messageDraftingSkillbook, replyHandlingSkillbook, schedulingSkillbook } from './_lib/skillbook-defaults.js';

export default async function handle(_command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const seeded = [];
    for (const record of [messageDraftingSkillbook(), replyHandlingSkillbook(), schedulingSkillbook()]) {
      const id = record.skillbook_id;
      if (await tx.get('outbound_skillbooks', id)) continue;
      record.created_at_ms = now;
      record.updated_at_ms = now;
      await tx.put('outbound_skillbooks', record);
      seeded.push(id);
    }
    return { ok: true, seeded };
  });
}
