import { getDatabase } from './businessOsDataSource.js';
import { tool, z } from './ctoxCommandAdapter.js';

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
  const rawResponse = await llmChat({
    agent: 'matcher',
    messages: [
      {
        role: 'system',
        content: 'You are a deterministic requirement matching engine. Return only valid JSON.'
      },
      {
        role: 'user',
        content: buildMatchPrompt(requirementText, objectText)
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
        payload: { requirementId: requirementId, objectId: objectId, itemsCount: items.length },
        at: nowIso
      }],
      items: items.map((item, index) => ({
        ...item,
        id: `${matchId}|${item.requirementId || `REQ-${index + 1}`}`,
        sourceId: sourceId || requirement.sourceId || '',
        requirementId,
        objectId,
        requirementId: item.requirementId || `REQ-${index + 1}`,
        createdAt: nowIso,
        updatedAt: nowIso,
        priorityKey: prioritySortKey(item.priority),
        matchLevelKey: matchLevelSortKey(item.matchLevel),
        matchScoreKey: Math.round(clamp01(item.matchScore) * 100)
      })),
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

export async function shortlistObjectsForRequirement({
  requirementId,
  objectIds = [],
  topN = 5
}) {
  const ids = Array.isArray(objectIds) ? objectIds.filter(Boolean) : [];
  return {
    requirementId,
    consideredObjectIds: ids,
    skippedAlreadyMatchedIds: [],
    shortlist: ids.slice(0, topN).map((objectId) => ({
      objectId,
      score: 0,
      reason: 'Basic mode: bulk scoring is disabled.'
    }))
  };
}

export async function shortlistRequirementsForObject({
  objectId,
  requirementIds = [],
  topN = 5
}) {
  const ids = Array.isArray(requirementIds) ? requirementIds.filter(Boolean) : [];
  return {
    objectId,
    consideredRequirementIds: ids,
    skippedInvalidRequirementIds: [],
    shortlist: ids.slice(0, topN).map((requirementId) => ({
      requirementId,
      score: 0,
      reason: 'Basic mode: bulk scoring is disabled.'
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
    fieldBlock('Muss-Kriterien', parsed.objectRequirements || requirement?.objectRequirements),
    fieldBlock('Kann-Kriterien', arrayText(parsed.benefits || requirement?.benefits)),
    fieldBlock('Rohtext', requirementSource?.rawText)
  ];
  return parts.filter(Boolean).join('\n\n').trim();
}

function buildObjectText(object) {
  const additional = Array.isArray(object?.additional) ? object.additional : [];
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
    fieldBlock('Skills', arrayText(object?.skills)),
    fieldBlock('Sprachen', arrayText(object?.languages?.map((lang) => [lang.code, lang.level].filter(Boolean).join(' ')))),
    fieldBlock('Zusammenfassung', object?.summary || object?.objectText || object?.rawText),
    fieldBlock('Weitere Evidenz', additionalText)
  ];
  return parts.filter(Boolean).join('\n\n').trim();
}

function buildMatchPrompt(requirementText, objectText) {
  return `
Vergleiche eine strukturierte Anforderung mit einem strukturierten Objekt.

Antworte nur mit einem JSON-Objekt:
{
  "items": [
    {
      "requirementId": "REQ-1",
      "title": "string",
      "dimension": "education | experience | skill | language | other",
      "priority": "base | performance | enthusiasm",
      "matchLevel": "full | partial | none",
      "matchScore": 0.0,
      "requirementSnippet": "string",
      "objectSnippet": "string",
      "explanation": "string"
    }
  ]
}

Regeln:
- Keine weiteren JSON-Keys.
- Leite Anforderungen nur aus der Anforderung ab.
- Nutze Evidenz aus beiden Seiten.
- matchScore ist 0.0 bis 1.0.
- Schreibe explanation in der Sprache der Anforderung.

ANFORDERUNG:
<<<REQUIREMENT_START>>>
${requirementText}
<<<REQUIREMENT_END>>>

OBJEKT:
<<<OBJECT_START>>>
${objectText}
<<<OBJECT_END>>>
`.trim();
}

function normalizeMatchItems(items) {
  const allowedDimensions = new Set(['education', 'experience', 'skill', 'language', 'other']);
  const allowedLevels = new Set(['full', 'partial', 'none']);
  return (Array.isArray(items) ? items : []).map((item, index) => ({
    requirementId: String(item?.requirementId || `REQ-${index + 1}`),
    title: String(item?.title || `Anforderung ${index + 1}`),
    dimension: allowedDimensions.has(item?.dimension) ? item.dimension : 'other',
    priority: normalizePriority(item?.priority),
    matchLevel: allowedLevels.has(item?.matchLevel) ? item.matchLevel : 'none',
    matchScore: clamp01(item?.matchScore),
    requirementSnippet: String(item?.requirementSnippet || ''),
    objectSnippet: String(item?.objectSnippet || ''),
    explanation: String(item?.explanation || '')
  }));
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
