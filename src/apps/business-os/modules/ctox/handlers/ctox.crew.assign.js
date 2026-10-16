// ctox.crew.assign – Port von crew_commands.rs:194-199 + crew::assign_member_before_lease
// (src/core/crew/mod.rs:354-380). Die Fachregel (nur aktive Mitglieder; nur
// unverleaste pending|blocked-Aufgaben) liegt hier. Die Bindung an die Task ist ein
// atomarer Claim über das generische Warteschlangen-Primitiv
// ctx.cap.agent.queue.task.claim (opaker Halter = Mitglieds-ID); die Zuordnung
// selbst führt die App-Collection ctox_crew_assignments (id = Task-ID).
import { fail } from './_lib/errors.js';
import { CREW_ASSIGNMENTS, CREW_MEMBERS, isoMillis, requireText, runCrewControl } from './_lib/crew.js';

// ref: src/core/crew/mod.rs:372 – route_status IN ('pending','blocked') AND lease_owner IS NULL
export const ASSIGNABLE_ROUTE_STATUSES = ['pending', 'blocked'];

export default async function handle(command, ctx) {
  return runCrewControl(command, ctx, async (p, now) => {
    const task = requireText(p, 'task_id');
    const member = requireText(p, 'member_id');
    // ref: src/core/crew/mod.rs:362-369 – EXISTS(… WHERE id=?1 AND archived=0)
    const current = await ctx.get(CREW_MEMBERS, member);
    if (!current || current.archived !== false) throw fail('not_found', 'active member not found');
    // ref: src/core/crew/mod.rs:370-378 – atomares UPDATE mit Bedingung; changed != 1 → Fehler.
    const claim = await ctx.cap.agent.queue.task.claim({
      task_id: task,
      holder: member,
      route_status_in: ASSIGNABLE_ROUTE_STATUSES,
    });
    if (!claim || claim.changed !== 1) {
      throw fail('conflict', 'assignment requires an unleased pending or blocked task');
    }
    await ctx.put(CREW_ASSIGNMENTS, {
      id: task,
      task_id: task,
      member_id: member,
      assigned_at: now,
      updated_at_ms: isoMillis(now),
    });
    return { task_id: task, member_id: member };
  });
}
