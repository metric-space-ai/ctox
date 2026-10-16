// Agenten-Simulation (v4/tools/agent-sim) gegen das echte Paket modules/news: Skill-Aufloesung aus skills/,
// Templates per ctx.asset aus prompts/, Rueckruf-Zustellung an handlers/<on_complete|on_failed>.js.
import { APP_DIR } from './_ctox-shared.mjs';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';
import { setup } from './_fixtures.mjs';

export async function world({ seed = {}, commands = null } = {}) {
  const t = await setup({ seed, commands });
  t.ctx.cap = {}; // die Simulation haengt agent.task an; andere Faehigkeiten braucht der Ablauf nicht
  const sim = createAgentTaskSim({ appId: 'news', appDir: APP_DIR, ctxFactory: () => t.ctx });
  const attach = (command) => sim.attach(t.ctx, command);
  return { t, sim, ctx: t.ctx, attach };
}
