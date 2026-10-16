// research.systematic.run – Aufnahme (Kern §16.30). Nativ gab es keinen eigenen Arm: der Typ ging über
// record_command in die Queue (Skill systematic-research) und wurde nach der Agenten-Antwort nativ abgeschlossen.
// Jetzt: Template prompts/systematic-run.md bzw. prompts/graph-gap.md (research_mode targeted_graph_gap) füllen,
// agent.task.enqueue mit dem Core-System-Skill systematic-research, Rückruf research.systematic.run.complete bzw.
// research.systematic.run.failed.
import { enqueueSystematicRun } from './_lib/enqueue.js';

export default async function handle(command, ctx) {
  return enqueueSystematicRun(command, ctx);
}
