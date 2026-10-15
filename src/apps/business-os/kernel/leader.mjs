import { StaleLeader } from './values.mjs';

/** Must be called in the SAME transaction as the effect, never before opening it. */
export async function assertLeader(t, isLeader, generation) {
  const stored = (await t.get('meta', 'leader_gen'))?.value ?? 0;
  if (!isLeader || stored !== generation) throw new StaleLeader('leader_gen expired');
}
export async function acquireGeneration(db) {
  return db.tx(async (t) => {
    const generation = ((await t.get('meta', 'leader_gen'))?.value ?? 0) + 1;
    await t.put('meta', { key: 'leader_gen', value: generation });
    return generation;
  });
}
