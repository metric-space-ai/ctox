import { getDatabase } from './businessOsDataSource.js';
import { tool, z } from './ctoxCommandAdapter.js';
import { getActiveMatchingDefinition } from './matchingDefinition.js';
import { evaluateKnockouts, rankShortlist } from '../core/screening.js';

function clamp01(value) {
  const num = Number(value);
  if (!Number.isFinite(num)) return 0;
  return Math.max(0, Math.min(1, num));
}

function normalizePriority(priority) {
  if (priority === 'base' || priority === 'performance' || priority === 'enthusiasm') return priority;
  return 'performance';
}

function priorityWeight(priority) {
  switch (normalizePriority(priority)) {
    case 'base':
      return 1.35;
    case 'performance':
      return 1;
    case 'enthusiasm':
      return 0.65;
    default:
      return 1;
  }
}

export function computeTotalMatchScoreFromItems(items) {
  const arr = Array.isArray(items) ? items : [];
  if (!arr.length) return 0;

  let weighted = 0;
  let weights = 0;
  let baseMissing = 0;
  let baseTotal = 0;

  for (const item of arr) {
    const priority = normalizePriority(item?.priority);
    const score = clamp01(item?.matchScore);
    const weight = priorityWeight(priority);
    weighted += score * weight;
    weights += weight;
    if (priority === 'base') {
      baseTotal += 1;
      if (score < 0.55) baseMissing += 1;
    }
  }

  const basePenalty = baseTotal ? (baseMissing / baseTotal) * 0.18 : 0;
  return Math.round(clamp01((weighted / Math.max(1, weights)) - basePenalty) * 100);
}

export async function recomputeAllMatchScoresOnce() {
  const db = await getDatabase();
  if (!db?.matches) throw new Error('matching results collection not available');

  const docs = await db.matches.find({ selector: {} }).exec();
  let updated = 0;
  const nowIso = new Date().toISOString();

  for (const docRx of docs) {
    const doc = docRx.toJSON();
    const score = computeTotalMatchScoreFromItems(doc.items);
    if (doc.score === score) continue;
    await updateDoc(docRx, { score, scoreKey: score, updatedAt: nowIso });
    updated += 1;
  }

  return { updated, total: docs.length };
}

export async function computeRequirementMatch({
  llmChat,
  sourceId,
  requirementId,
  objectId,
  persist = true
}) {
  if (typeof llmChat !== 'function') throw new Error('computeRequirementMatch: llmChat is required');
  if (!requirementId || !objectId) throw new Error('computeRequirementMatch: requirementId and objectId are required');

  const db = await getDatabase();
  if (!db?.requirements || !db?.objects) throw new Error('matching database is not available');

  const requirementRx = await db.requirements.findOne({ selector: { id: requirementId } }).exec();
  const objectRx = await db.objects.findOne({ selector: { id: objectId } }).exec();
  if (!requirementRx) throw new Error(`Requirement not found: ${requirementId}`);
  if (!objectRx) throw new Error(`Object not found: ${objectId}`);

  const requirement = requirementRx.toJSON();
  const object = objectRx.toJSON();
  const requirementSource = await loadNewestRequirementSource(db, requirementId);
  const requirementText = buildRequirementText(requirement, requirementSource);
  const objectText = buildObjectText(object);
  if (!requirementText) throw new Error('Requirement content is empty');
  if (!objectText) throw new Error('Object content is empty');

  const matchId = `${sourceId || requirement.sourceId || 'requirement'}|${requirementId}|${objectId}`;
  const definition = getActiveMatchingDefinition();
  const definitionId = definition?.id || 'generic_matching.v1';
  const schemaVersion = definition?.engine?.version || 'generic_matching.v1';
  const rawResponse = await llmChat({
    agent: 'planner',
    messages: [
      {
        role: 'system',
        content: [
          definition.prompts?.system,
          definition.prompts?.domainSystem,
          `Active matching definition: ${definitionId}`
        ].filter(Boolean).join('\n')
      },
      {
        role: 'user',
        content: buildMatchPrompt(requirementText, objectText, definition)
      }
    ]
  }, {
    module: 'matching',
    commandType: 'matching.match',
    recordId: matchId,
    matchId,
    businessContext: {
      action: 'match',
      requirementId: requirementId,
      objectId: objectId,
      definitionId,
      schemaVersion,
      output_collection: 'matching_results'
    }
  });

  const parsed = parseJsonObject(rawResponse);
  const items = normalizeMatchItems(parsed.items);
  const score = computeTotalMatchScoreFromItems(items);
  const nowIso = new Date().toISOString();
  let persisted = false;

  if (persist) {
    if (!db.matches) throw new Error('matching results collection not available');
    await db.matches.atomicUpsert({
      id: matchId,
      definitionId,
      schemaVersion,
      sourceId: sourceId || requirement.sourceId || '',
      requirementId,
      objectId,
      active: true,
      removed: false,
      progress: 10,
      status: 'prematch',
      score,
      scoreKey: score,
      notes: '',
      interview: { attendees: [], reminders: [] },
      events: [{
        type: 'match.created',
        payload: { requirementId: requirementId, objectId: objectId, definitionId, schemaVersion, itemsCount: items.length },
        at: nowIso
      }],
      items: items.map((item, index) => {
        const itemRequirementId = item.requirementId || `REQ-${index + 1}`;
        return {
          ...item,
          id: `${matchId}|${itemRequirementId}`,
          definitionId,
          schemaVersion,
          sourceId: sourceId || requirement.sourceId || '',
          matchRequirementId: requirementId,
          matchObjectId: objectId,
          objectId,
          requirementId: itemRequirementId,
          createdAt: nowIso,
          updatedAt: nowIso,
          priorityKey: prioritySortKey(item.priority),
          matchLevelKey: matchLevelSortKey(item.matchLevel),
          matchScoreKey: Math.round(clamp01(item.matchScore) * 100)
        };
      }),
      createdAt: nowIso,
      updatedAt: nowIso,
      activeKey: 1
    });
    persisted = true;
  }

  return {
    match: { items },
    score,
    persisted,
    matchId
  };
}

export async function generateSyntheticRequirementFromCv() {
  return {
    objectId: '',
    objectText: '',
    record: {
      about_source: '',
      about_role: '',
      benefits: '',
      object_requirements: '',
      closing_notes: '',
      internal_reasoning: null
    }
  };
}

// Bulk shortlisting ranks the considered pool by the real, persisted per-pair
// match score (computeRequirementMatch produces these) and applies optional
// must-have knock-out rules. Candidates with no match yet rank last as
// "noch nicht bewertet" instead of the old score:0 stub.
async function loadRequirementMatchScores(db, requirementId) {
  const byObject = new Map();
  if (!db?.matches) return byObject;
  const docs = await db.matches.find({ selector: { requirementId } }).exec();
  for (const docRx of docs) {
    const doc = docRx.toJSON();
    if (doc.removed) continue;
    const score = Number.isFinite(doc.score) ? doc.score : computeTotalMatchScoreFromItems(doc.items);
    byObject.set(doc.objectId, score);
  }
  return byObject;
}

async function loadObjectsByIds(db, ids) {
  const byId = new Map();
  if (!db?.objects) return byId;
  for (const id of ids) {
    const rx = await db.objects.findOne({ selector: { id } }).exec();
    if (rx) byId.set(id, rx.toJSON());
  }
  return byId;
}

export async function shortlistObjectsForRequirement({
  requirementId,
  objectIds = [],
  topN = 5,
  knockoutRules = []
}) {
  const ids = Array.isArray(objectIds) ? objectIds.filter(Boolean) : [];
  const db = await getDatabase();
  const scoreByObject = await loadRequirementMatchScores(db, requirementId);
  const objectById = knockoutRules.length ? await loadObjectsByIds(db, ids) : new Map();

  const scored = ids.map((objectId) => {
    const ko = knockoutRules.length
      ? evaluateKnockouts(objectById.get(objectId) || {}, knockoutRules)
      : { passed: true, failed: [] };
    return {
      objectId,
      score: scoreByObject.has(objectId) ? scoreByObject.get(objectId) : null,
      evaluated: scoreByObject.has(objectId),
      knockoutFailed: !ko.passed,
      knockoutReasons: ko.failed
    };
  });

  return {
    requirementId,
    consideredObjectIds: ids,
    knockedOut: scored.filter((s) => s.knockoutFailed).map((s) => ({ objectId: s.objectId, reasons: s.knockoutReasons })),
    shortlist: rankShortlist(scored, { topN })
  };
}

export async function shortlistRequirementsForObject({
  objectId,
  requirementIds = [],
  topN = 5
}) {
  const ids = Array.isArray(requirementIds) ? requirementIds.filter(Boolean) : [];
  const db = await getDatabase();
  const scoreByRequirement = new Map();
  if (db?.matches) {
    const docs = await db.matches.find({ selector: { objectId } }).exec();
    for (const docRx of docs) {
      const doc = docRx.toJSON();
      if (doc.removed) continue;
      const score = Number.isFinite(doc.score) ? doc.score : computeTotalMatchScoreFromItems(doc.items);
      scoreByRequirement.set(doc.requirementId, score);
    }
  }

  const scored = ids.map((requirementId) => ({
    objectId: requirementId,
    score: scoreByRequirement.has(requirementId) ? scoreByRequirement.get(requirementId) : null,
    evaluated: scoreByRequirement.has(requirementId)
  }));

  return {
    objectId,
    consideredRequirementIds: ids,
    shortlist: rankShortlist(scored, { topN }).map((entry) => ({
      requirementId: entry.objectId,
      score: entry.score,
      rank: entry.rank,
      reason: entry.reason
    }))
  };
}

export const computeRequirementMatchTool = tool({
  name: 'compute_requirement_match',
  description: 'Queues one structured requirement/object match through the CTOX harness.',
  parameters: z.object({
    sourceId: z.string().optional(),
    requirementId: z.string(),
    objectId: z.string(),
    persist: z.boolean().optional()
  }),
  async execute({ sourceId, requirementId, objectId, persist }, context) {
    return computeRequirementMatch({
      llmChat: context?.llmChat,
      sourceId,
      requirementId,
      objectId,
      persist: typeof persist === 'boolean' ? persist : true
    });
  }
});

async function loadNewestRequirementSource(db, requirementId) {
  if (!db?.requirementSources) return null;
  const docs = await db.requirementSources.find({ selector: { requirementId } }).exec();
  const rows = docs.map((doc) => doc.toJSON());
  rows.sort((a, b) => String(b.publishAt || b.updatedAt || b.createdAt || '').localeCompare(String(a.publishAt || a.updatedAt || a.createdAt || '')));
  return rows[0] || null;
}

function buildRequirementText(requirement, requirementSource) {
  const parsed = requirementSource?.parsed || {};
  const parts = [
    fieldBlock('Titel', requirement?.title),
    fieldBlock('Quelle', requirementSource?.sourceUrl || requirementSource?.source || requirement?.sourceUrl),
    fieldBlock('Zusammenfassung', requirement?.summary),
    fieldBlock('Über die Organisation', parsed.aboutSource || requirement?.aboutSource),
    fieldBlock('Anforderung / Rolle', parsed.aboutRole || requirement?.aboutRole || requirementSource?.rawText),
    fieldBlock('Aufgaben', parsed.responsibilities || requirement?.responsibilities),
    fieldBlock('Muss-Kriterien', parsed.objectRequirements || requirement?.objectRequirements),
    fieldBlock('Anforderungen Liste', parsed.requirements || requirement?.requirements),
    fieldBlock('Kann-Kriterien', arrayText(parsed.benefits || requirement?.benefits)),
    fieldBlock('Rohtext', requirementSource?.rawText)
  ];
  return parts.filter(Boolean).join('\n\n').trim();
}

function buildObjectText(object) {
  const additional = Array.isArray(object?.additional) ? object.additional : [];
  const topLevelEducation = Array.isArray(object?.education) ? object.education : [];
  const topLevelExperience = Array.isArray(object?.experience) ? object.experience : [];
  const additionalText = additional
    .map((entry) => {
      const value = typeof entry?.value === 'string' ? entry.value : JSON.stringify(entry?.value || '');
      return [entry?.label || entry?.key, value].filter(Boolean).join(': ');
    })
    .filter(Boolean)
    .join('\n');

  const parts = [
    fieldBlock('Name', object?.name),
    fieldBlock('Aktuelle Rolle', object?.currentRole),
    fieldBlock('Ziel / Wunsch', object?.desiredPosition),
    fieldBlock('Region', object?.region || object?.location),
    fieldBlock('Abschluss', object?.highestDegree || object?.degree),
    fieldBlock('Ausbildung', topLevelEducation.map((entry) => [
      entry?.degree,
      entry?.major,
      entry?.institution,
      entry?.location,
      [entry?.start_date, entry?.end_date].filter(Boolean).join(' - '),
      ...(Array.isArray(entry?.details) ? entry.details : [])
    ].filter(Boolean).join(' - '))),
    fieldBlock('Berufserfahrung', topLevelExperience.map((entry) => [
      entry?.job_title || entry?.requirement_title,
      entry?.employer,
      entry?.location,
      [entry?.start_date, entry?.end_date].filter(Boolean).join(' - '),
      ...(Array.isArray(entry?.job_description) ? entry.job_description : []),
      ...(Array.isArray(entry?.requirement_description) ? entry.requirement_description : [])
    ].filter(Boolean).join(' - '))),
    fieldBlock('Skills', arrayText(object?.skills)),
    fieldBlock('Sprachen', arrayText(object?.languages?.map((lang) => [lang.code, lang.level].filter(Boolean).join(' ')))),
    fieldBlock('Zusammenfassung', object?.summary || object?.objectText || object?.rawText),
    fieldBlock('Weitere Evidenz', additionalText)
  ];
  return parts.filter(Boolean).join('\n\n').trim();
}

function buildMatchPrompt(requirementText, objectText, definition = getActiveMatchingDefinition()) {
  const promptDef = definition?.prompts || {};
  const sourceTextLabel = promptDef.sourceTextLabel || 'Source';
  const objectTextLabel = promptDef.objectTextLabel || 'Object';
  const task = promptDef.task || 'Compare the source record with the object record and produce structured match items.';
  return `
Active matching definition:
- id: ${definition?.id || 'generic_matching.v1'}
- title: ${definition?.title || 'Generic Matching'}
- engine: ${definition?.engine?.version || 'generic_matching.v1'}

Domain setup:
${promptDef.domainSystem || 'You are a generic matching engine.'}

Your task:
${task}

Output format:
You MUST respond with a single valid JSON object with the following structure:
{
  "items": [
    {
      "requirementId": "string",
      "title": "string",
      "dimension": "education | experience | skill | language | other",
      "priority": "base | performance | enthusiasm",
      "matchLevel": "full | partial | none",
      "matchScore": number,
      "jobSnippet": "string",
      "cvSnippet": "string",
      "explanation": "string"
    }
  ]
}

IMPORTANT RULE (NO SCHEMA CHANGES):
Do NOT add any new JSON keys. The output format must remain EXACTLY as defined above.

Core scoring principles (IMPORTANT):
1) Studentische Tätigkeiten vs. echte Berufserfahrung (ANTI-OVER-SCORING):
- Treat student roles ("Werkstudent", "Praktikum", "Hiwi", "studentische Hilfskraft", "Abschlussarbeit", "Trainee/Intern") as valuable but NOT equivalent to full professional experience.
- If the requirement is explicitly "Berufserfahrung X Jahre" or "mehrjährige Berufserfahrung":
  - Student work can support PARTIAL fulfillment, but must be capped.
  - Default cap for student-only evidence: matchScore max 0.55 for that requirement, unless there is also clear non-student professional experience of Thesis Works that exactly match the topic.
  - If student work is very long and very relevant (e.g., >18–24 months highly relevant, clear responsibilities, tools, outcomes), allow up to 0.65 — still not "full" unless there is actual post-study professional employment in similar scope.
- If the requirement explicitly accepts student background (e.g., "erste praktische Erfahrung", "Praktika/Werkstudententätigkeit willkommen", "Einsteiger"):
  - Student work may score higher and can become "full" if it matches the described expectations.
- Do NOT harshly penalize missing professional years if the role is junior/entry: keep matchScore in a fair partial range (e.g., 0.45–0.70) depending on relevance and recency.

2) Abgeschlossenes Studium vs. kurz vor Abschluss (NEAR-COMPLETION RULE):
- If a base requirement is "abgeschlossenes Studium" (or equivalent) and the CV clearly indicates the candidate is close to completion (e.g., "in den letzten Zügen", "Abschluss in MM/YYYY", "Masterarbeit/Bachelorarbeit läuft", "alle Module abgeschlossen", "Graduation expected"):
  - Treat it as largely fulfilled: matchScore should be 0.80–0.90 by default.
  - Use "partial" (not "none") unless the job explicitly requires the degree already in hand by start date AND the CV timing clearly conflicts.
- If the job requires degree "zwingend bei Eintritt/Start" or "Urkunde erforderlich" and the candidate finishes later than the stated start:
  - Score lower and consider adding an "availability" conflict item only if the timing conflict is CLEAR.

3) Overqualification / Level & Scope sanity (NO MASTER-FOR-FACHARBEITER):
- Never implicitly assume that a higher degree automatically improves fit for roles that are clearly non-academic or shopfloor/clerical unless the job explicitly welcomes it.
- If the job is clearly a Facharbeiter/Sachbearbeiter/Assistant/Techniker role with no study requirement and the CV shows a clear high-academic/high-seniority profile:
  - Do NOT inflate matchScore on education items just because the candidate has a Master.
  - Instead, consider a "level_scope" conflict item ONLY if the incompatibility is CLEAR (see conflict rules), otherwise keep scoring neutral.
- If candidate qualifies on skills but is likely over-scoped, reflect this via:
  - lower matchScore on "role-level fit" related items (dimension "other" via normal requirement items if the job states level expectations),
  - and/or a conflict item when clearly inferable (see section A: level_scope).

4) Automotive leadership reality check (PROJECT LEADERSHIP ≠ PEOPLE MANAGEMENT):
- In Automotive contexts, do NOT treat "Projektleitung/Teilprojektleitung/Project Lead" as "Führungskraft" unless people management is explicitly stated.
- For leadership requirements:
  - If the job asks for "Führungskraft", "disziplinarische Führung", "Personalverantwortung", "Teamleitung", "Line Manager":
    - Only score "full" if the CV shows explicit people management (team size, direct reports, hiring, disciplinary leadership, performance reviews, budget responsibility).
    - Project leadership without disciplinary leadership should be "partial" (often 0.45–0.70 depending on strength).
  - If the job asks for "Projektleitung" (without explicit disciplinary leadership):
    - Strong project leadership evidence can be "full".
- Also apply seniority plausibility:
  - If the job expects true leadership experience and the CV indicates clear Berufseinsteiger/junior profile, do NOT score high; keep it partial/none as appropriate.

5) Verfügbarkeit ambiguity (START DATE vs TRAVEL FLEXIBILITY):
- Distinguish two different meanings:
  A) "kurzfristige Verfügbarkeit" meaning: candidate can START quickly (notice period, start date).
  B) "kurzfristig verfügbar" meaning: candidate can be sent on short-notice travel/assignments, flexibility for deployment.
- When extracting requirements from the job:
  - Create separate items if both meanings appear or are strongly implied (e.g., "Start ASAP" AND "Reisebereitschaft kurzfristig").
- Scoring:
  - If job focuses on start date and CV provides notice period/start date: score based on that.
  - If job focuses on travel flexibility and CV provides travel willingness/constraints: score based on that.
  - If the job wording is ambiguous and CV does not clarify, avoid over-penalizing: keep a moderate partial score (e.g., 0.50–0.70) and explain the ambiguity positively.

Base-factor conflict ITEMS (titles are used as keys by the UI):
In addition to normal requirement items, you MUST run the following 8 base-factor conflict checks.
If (and only if) a CLEAR conflict is inferable from job description + CV, you MUST add an extra item with:
- priority: "base"
- dimension: "other"
- title: EXACTLY one of these strings (must match character-by-character):
  1) "level_scope"
  2) "compensation_band"
  3) "location_work_model"
  4) "career_path"
  5) "domain_industry"
  6) "role_definition"
  7) "availability"
  8) "eligibility_restriction"

These special conflict items MUST ONLY appear when there is a conflict. If no conflict is clearly inferable, OMIT them completely.
They are metadata items for UI rendering; they still must use the normal fields.

How to score these conflict items:
- matchScore MUST remain a normal 0.0..1.0 score like any other item.
- Set matchScore to reflect the SEVERITY/LIKELIHOOD of the conflict:
  - 1.0 = very strong / very likely conflict (clearly blocking)
  - 0.7 = strong conflict
  - 0.5 = moderate conflict
  - 0.3 = weak but present conflict
- matchLevel:
  - "full" = strong conflict clearly present
  - "partial" = some conflict signals, but not totally conclusive
  - "none" = do NOT use for these items (if no conflict, omit the item instead)

Evidence requirement:
- For every conflict item you add, jobSnippet MUST contain the job-side evidence and cvSnippet MUST contain the CV-side evidence (as connected substrings where possible).

Conflict detection rules (only when CLEAR):

A) level_scope
Flag when job is clearly clerical/IC level (e.g., "Sachbearbeiter", "Assistant", "Mitarbeiter", "Facharbeiter", "Specialist", "Operator", "Montage", "Produktion")
AND the CV shows strong long-term leadership/executive scope (e.g., titles "Leiter", "Head of", "Director", "Manager" with people management OR explicit "disziplinarische Führung" with team size/budget/overall responsibility).
Important nuance:
- Do NOT flag solely because of a Master/PhD. Degree alone is not a level_scope conflict.
- Do NOT flag solely because of project leadership. Project lead ≠ line leadership.

B) compensation_band
Flag only if job strongly implies a lower/tight band (e.g., tariff group, explicitly junior/clerical role)
AND CV strongly implies much higher seniority/comp expectations (e.g., executive compensation signals, very senior titles with long tenure).

C) location_work_model
Flag if job requires on-site/shift/travel/relocation AND CV/cover letter explicitly restricts this.

D) career_path
Flag if job is clearly leadership track (line management) but CV/cover letter clearly indicates specialist/IC preference, or vice versa.

E) domain_industry
Flag if job is strongly domain/regulatory-specific AND CV shows different domain with no transferable evidence.

F) role_definition
Flag if job expects hands-on operational execution but CV is almost entirely management-only (or the reverse), clearly incompatible.

G) availability
Flag if job needs immediate start/full-time/fixed schedule and CV clearly states conflicting notice period/start/part-time.
Important nuance:
- Do NOT use availability for travel-flexibility unless the job clearly demands travel/shift and CV clearly blocks it (that can also be location_work_model if appropriate).
- If job says "kurzfristig verfügbar" but meaning is ambiguous, only flag conflict when the CV clearly contradicts BOTH reasonable interpretations.

H) eligibility_restriction
Flag when the job clearly includes lawful access, citizenship, nationality, gender-for-duty, security clearance, export-control, defence-sector, or comparable eligibility restrictions
AND the CV clearly conflicts with that restriction or clearly lacks the required lawful eligibility signal.
Important nuance:
- Use this ONLY for explicit, job-relevant restrictions that are stated in the job text.
- Do NOT invent this conflict from vague culture fit or language preferences.

All other existing rules still apply:
- Identify requirements and create items.
- Use dimension/priority as specified.
- explanation must be in the job description language and positively phrased.
- Root must be exactly { "items": [...] } and nothing else.
- No markdown, no extra text.

${sourceTextLabel}:
<<<SOURCE_RECORD_START>>>
${requirementText}
<<<SOURCE_RECORD_END>>>

${objectTextLabel}:
<<<OBJECT_RECORD_START>>>
${objectText}
<<<OBJECT_RECORD_END>>>
`.trim();
}

function normalizeMatchItems(items) {
  const allowedDimensions = new Set(['education', 'experience', 'skill', 'language', 'other']);
  const allowedLevels = new Set(['full', 'partial', 'none']);
  return (Array.isArray(items) ? items : []).map((item, index) => {
    const jobSnippet = String(item?.jobSnippet || item?.requirementSnippet || '');
    const cvSnippet = String(item?.cvSnippet || item?.objectSnippet || '');
    return {
      requirementId: String(item?.requirementId || `REQ-${index + 1}`),
      title: String(item?.title || `Anforderung ${index + 1}`),
      dimension: allowedDimensions.has(item?.dimension) ? item.dimension : 'other',
      priority: normalizePriority(item?.priority),
      matchLevel: allowedLevels.has(item?.matchLevel) ? item.matchLevel : 'none',
      matchScore: clamp01(item?.matchScore),
      jobSnippet,
      cvSnippet,
      requirementSnippet: jobSnippet,
      objectSnippet: cvSnippet,
      explanation: String(item?.explanation || '')
    };
  });
}

function parseJsonObject(raw) {
  if (!raw || typeof raw !== 'string') throw new Error('CTOX matcher returned an empty response');
  let text = raw.trim();
  const first = text.indexOf('{');
  const last = text.lastIndexOf('}');
  if (first >= 0 && last > first) text = text.slice(first, last + 1);
  return JSON.parse(text);
}

function fieldBlock(label, value) {
  const text = arrayText(value);
  return text ? `${label}:\n${text}` : '';
}

function arrayText(value) {
  if (Array.isArray(value)) return value.map((item) => typeof item === 'string' ? item : JSON.stringify(item)).filter(Boolean).join('\n');
  if (value == null) return '';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value).trim();
}

function prioritySortKey(priority) {
  switch (normalizePriority(priority)) {
    case 'base':
      return 2;
    case 'performance':
      return 1;
    case 'enthusiasm':
      return 0;
    default:
      return 0;
  }
}

function matchLevelSortKey(level) {
  switch (level) {
    case 'full':
      return 2;
    case 'partial':
      return 1;
    default:
      return 0;
  }
}

async function updateDoc(docRx, patch) {
  if (typeof docRx.atomicPatch === 'function') return docRx.atomicPatch(patch);
  if (typeof docRx.atomicUpdate === 'function') return docRx.atomicUpdate((prev) => ({ ...prev, ...patch }));
  if (typeof docRx.incrementalModify === 'function') return docRx.incrementalModify((prev) => ({ ...prev, ...patch }));
  throw new Error('No supported update method on matching result docs');
}
