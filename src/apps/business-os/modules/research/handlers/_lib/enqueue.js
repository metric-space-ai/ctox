// Agenten-Aufträge der research-App (Kern §16.30): Template aus prompts/ füllen, agent.task.enqueue mit Skill und
// Rückruf an die eigene App. Ersetzt store.rs::record_command + suggested_skill_for_command (:27735-27770) und
// command_prompt (:27009); die Anweisungstexte standen bis SKL-B2 als Literale in index.js (:2586-2598, :4325-4344,
// knowledgeRefreshPayload :4768-4779) und liegen jetzt wörtlich in prompts/*.md.
import { fillTemplate, loadPromptTemplate, renderPrompt } from 'ctox:shared/prompt-template.js';
import { invalid } from './errors.js';

// Core-System-Skill (src/skills/system/research/systematic-research): generische Recherche-Methodik, die auch
// documents (research.systematic.report.create) nutzt – bleibt im Core, Aufruf ohne Namensraum.
export const SYSTEMATIC_RESEARCH_SKILL = 'systematic-research';
// Eigener Skill im Paket (skills/knowledge-refresh/SKILL.md); ersetzt den Phantom-Namen der Core-Heuristik.
export const KNOWLEDGE_REFRESH_SKILL = 'research/knowledge-refresh';

export const RUN_COMPLETE = 'research.systematic.run.complete';
export const RUN_FAILED = 'research.systematic.run.failed';
export const REFRESH_COMPLETE = 'research.knowledge.refresh.complete';
export const REFRESH_FAILED = 'research.knowledge.refresh.failed';

const text = (v) => (typeof v === 'string' ? v.trim() : '');
const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const count = (v) => (Number.isFinite(Number(v)) ? Number(v) : null);

function required(values) {
  const missing = Object.entries(values)
    .filter(([k, v]) => v === null || v === undefined || (v === '' && !k.endsWith('_block') && k !== 'scoring_model'))
    .map(([k]) => k);
  if (missing.length) throw invalid(`missing ${missing.join(', ')}`, { missing });
  return values;
}

// Zeilen, die index.js früher per `.filter(Boolean)` wegließ, kommen als Block mit eigenem Zeilenende.
const block = (value) => (value ? `${value}\n` : '');
// Rust trimmte die Anweisung (command_instruction :26992); das Template endet mit einem Zeilenumbruch.
const finish = (rendered) => rendered.replace(/\s+$/, '');

function taskIdOf(command, p) {
  return text(command.record_id) || text(p.record_id) || text(p.research_brief?.task_id);
}

/** Werte für prompts/systematic-run.md (Hauptlauf) aus Payload + research_brief. */
export function systematicRunValues(command) {
  const p = command.payload || {};
  const brief = isObject(p.research_brief) ? p.research_brief : {};
  const plan = isObject(p.web_stack_plan) ? p.web_stack_plan : {};
  const scoring = Array.isArray(brief.scoring_model) ? brief.scoring_model : [];
  return required({
    task_title: text(brief.task_title),
    task_id: taskIdOf(command, p),
    research_run_id: text(p.research_run_id),
    research_command_id: text(p.research_command_id),
    discovery_depth: text(plan.depth),
    target_verified_sources: count(plan.target_verified_sources),
    minimum_candidate_sources: count(plan.minimum_candidate_sources),
    minimum_discovery_rounds: count(plan.minimum_discovery_rounds),
    minimum_scholarly_rounds: count(plan.minimum_scholarly_rounds),
    knowledge_domain: text(p.knowledge_domain),
    task_prompt_block: block(text(brief.task_prompt)),
    criteria_block: block(text(brief.criteria) ? `Kriterien:\n${brief.criteria}` : ''),
    scoring_model: scoring.map((axis) => `- ${axis.id}: ${axis.label}; weight=${axis.weight}`).join('\n'),
    axis_x: text(brief.axis_x),
    axis_y: text(brief.axis_y),
    known_source_count: Array.isArray(plan.exclude_urls) ? plan.exclude_urls.length : null,
    verified_source_count: count(brief.verified_source_count),
  });
}

/** Werte für prompts/graph-gap.md (gezielte Graph-Nachrecherche). */
export function graphGapValues(command) {
  const p = command.payload || {};
  const brief = isObject(p.research_brief) ? p.research_brief : {};
  const focus = isObject(p.graph_focus) ? p.graph_focus : {};
  const related = Array.isArray(focus.related_terms) ? focus.related_terms.filter((t) => text(t)) : [];
  return required({
    task_title: text(brief.task_title),
    research_run_id: text(p.research_run_id),
    research_command_id: text(p.research_command_id),
    focus: text(focus.label),
    related_block: block(related.length ? `Benachbarte Begriffe: ${related.join(', ')}` : ''),
    knowledge_domain: text(p.knowledge_domain),
    task_prompt_block: block(text(brief.task_prompt)),
  });
}

/** Werte für prompts/knowledge-refresh.md. */
export function knowledgeRefreshValues(command) {
  const p = command.payload || {};
  const brief = isObject(p.research_brief) ? p.research_brief : {};
  return required({
    task_title: text(brief.task_title),
    task_id: text(p.research_task_id) || taskIdOf(command, p),
    research_run_id: typeof p.research_run_id === 'string' ? p.research_run_id : null,
    research_command_id: text(p.research_command_id),
    knowledge_domain: text(p.knowledge_domain),
  });
}

const optional = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null);

/** Template-Name für research.systematic.run: Graph-Nachrecherche (research_mode) oder Hauptlauf. */
export function systematicRunPromptName(command) {
  return command?.payload?.research_mode === 'targeted_graph_gap' ? 'graph-gap' : 'systematic-run';
}

/**
 * Anweisung für research.systematic.run aus dem Template-Text (prompts/<systematicRunPromptName>.md) und dem Befehl
 * {record_id, payload}. Einzige Füllung für Handler und UI (RCH2: der Chat-Entwurf ist byte-gleich zur Anweisung,
 * die dieser Handler aus demselben Befehl erzeugt).
 */
export function systematicRunInstruction(template, command, values = systematicRunPromptValues(command)) {
  return finish(fillTemplate(template, values));
}

function systematicRunPromptValues(command) {
  return systematicRunPromptName(command) === 'graph-gap' ? graphGapValues(command) : systematicRunValues(command);
}

/** research.systematic.run: Hauptlauf oder Graph-Nachrecherche (research_mode), Skill systematic-research. */
export async function enqueueSystematicRun(command, ctx) {
  const p = command.payload || {};
  // Reihenfolge wie bisher: erst die Werte (fehlende → invalid), dann das Template laden.
  const values = systematicRunPromptValues(command);
  const instruction = systematicRunInstruction(await loadPromptTemplate(ctx, systematicRunPromptName(command)), command, values);
  const task = await ctx.cap.agent.task.enqueue({
    title: optional(p.title) ?? `Research · ${p.research_brief?.task_title}`,
    instruction,
    skill: SYSTEMATIC_RESEARCH_SKILL,
    thread_key: optional(p.thread_key),
    priority: optional(p.priority),
    payload: { ...p, business_command_id: command.id },
    idempotency_key: command.id,
    on_complete: RUN_COMPLETE,
    on_failed: RUN_FAILED,
  });
  return { ok: true, command_id: command.id, task_id: task?.task_id ?? null, status: 'queued' };
}

/** research.knowledge.refresh: Knowledge Book aus einem abgeschlossenen Lauf, eigener Skill knowledge-refresh. */
export async function enqueueKnowledgeRefresh(command, ctx) {
  const p = command.payload || {};
  const values = knowledgeRefreshValues(command);
  const instruction = finish(await renderPrompt(ctx, 'knowledge-refresh', values));
  const task = await ctx.cap.agent.task.enqueue({
    title: optional(p.title) ?? `Knowledge aktualisieren · ${values.task_title}`,
    instruction,
    skill: KNOWLEDGE_REFRESH_SKILL,
    thread_key: optional(p.thread_key),
    priority: optional(p.priority),
    payload: { ...p, business_command_id: command.id },
    idempotency_key: command.id,
    on_complete: REFRESH_COMPLETE,
    on_failed: REFRESH_FAILED,
  });
  return { ok: true, command_id: command.id, task_id: task?.task_id ?? null, status: 'queued' };
}
