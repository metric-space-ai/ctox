// ctox.crew.member.update – Port von crew_commands.rs:111-135 (4794af03d).
// Liest und schreibt das Mitglied in der App-Collection ctox_crew_members.
import { fail, invalid } from './_lib/errors.js';
import {
  CREW_MEMBERS,
  isoMillis,
  listMembers,
  parseSoul,
  parseSpecialties,
  requireText,
  runCrewControl,
  safeProse,
} from './_lib/crew.js';

export default async function handle(command, ctx) {
  return runCrewControl(command, ctx, async (p, now) => {
    const id = requireText(p, 'member_id');
    await ctx.transaction(async (tx) => {
      // ref: crew_commands.rs:113-116 – crew::members(…).find(id)
      const current = (await listMembers(tx)).find((m) => m.id === id);
      if (!current) throw fail('not_found', 'member not found');
      const member = { ...current };
      if (p.name !== undefined) {
        if (typeof p.name !== 'string') throw invalid('invalid name');
        if (!safeProse(p.name, 60)) throw invalid('invalid name');
        member.name = p.name.trim();
      }
      if (p.soul !== undefined) member.soul = parseSoul(p.soul);
      if (p.specialties !== undefined) member.specialties = parseSpecialties(p.specialties);
      if (p.archived !== undefined) {
        if (typeof p.archived !== 'boolean') throw invalid('archived must be boolean');
        member.archived = p.archived;
      }
      // ref: crew_commands.rs:133 – Rust schreibt nur name, soul, specialties, archived, updated_at.
      await tx.patch(CREW_MEMBERS, id, {
        name: member.name,
        soul: member.soul,
        specialties: member.specialties,
        archived: member.archived,
        updated_at: now,
        updated_at_ms: isoMillis(now),
      });
    });
    return { member_id: id };
  });
}
