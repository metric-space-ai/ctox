// ctox.crew.member.create – Port von crew_commands.rs:83-110 (4794af03d).
// Das Crew-Register ist die App-Collection ctox_crew_members; Prüfung,
// Normalisierung, Kapazität, ID und Persistenz liegen hier. Kapazitätszählung und
// Anlage laufen in einer ctx.transaction (Rust: COUNT + INSERT in einer Transaktion).
import { fail, invalid } from './_lib/errors.js';
import {
  CREW_COLORS,
  CREW_MEMBERS,
  CREW_SHAPES,
  defaultStats,
  isoMillis,
  parseSoul,
  parseSpecialties,
  requireText,
  runCrewControl,
  safeProse,
} from './_lib/crew.js';

export const CREW_MEMBER_CAPACITY = 1000;

export default async function handle(command, ctx) {
  return runCrewControl(command, ctx, async (p, now) => {
    const name = requireText(p, 'name');
    if (!safeProse(name, 60)) throw invalid('invalid member name');
    const shape = requireText(p, 'shape');
    if (!CREW_SHAPES.includes(shape)) throw invalid('invalid shape');
    const color = requireText(p, 'color');
    if (!CREW_COLORS.includes(color)) throw invalid('color is not a CREW_COLOR');
    if (p.soul === undefined) throw invalid('soul is required');
    const soul = parseSoul(p.soul);
    const specialties = parseSpecialties(p.specialties === undefined ? {} : p.specialties);
    const id = `crew:${newUuid(command, ctx)}`;
    const nowMs = isoMillis(now);
    await ctx.transaction(async (tx) => {
      // ref: crew_commands.rs:103-105 – COUNT(*) zählt auch archivierte Mitglieder.
      const count = (await tx.read(CREW_MEMBERS, {})).length;
      if (count >= CREW_MEMBER_CAPACITY) throw fail('quota', 'crew member capacity reached');
      // ref: crew_commands.rs:107-108 (INSERT INTO crew_members …, archived=0, created_at=updated_at=now)
      await tx.put(CREW_MEMBERS, {
        id,
        name,
        shape,
        color,
        created_at: now,
        created_at_ms: nowMs,
        archived: false,
        soul,
        specialties,
        stats: defaultStats(),
        // Laufzeitfelder wie die bisherige Projektion für ein neues Mitglied
        // (harness_cockpit_projections.rs:1570-1576: kein Lease, kein Fehlschlag → home).
        state: 'home',
        active_task_id: null,
        updated_at: now,
        updated_at_ms: nowMs,
      });
    });
    return { member_id: id };
  });
}

// Rust: uuid::Uuid::new_v4(). Handler-Global crypto.randomUUID bzw. ctx.uuid
// (§16.21); im Rückfall die Befehls-ID (eindeutig je Befehl, idempotent).
function newUuid(command, ctx) {
  const c = globalThis.crypto;
  if (c && typeof c.randomUUID === 'function') return c.randomUUID();
  if (typeof ctx.uuid === 'function') return ctx.uuid();
  return String(command.id);
}
