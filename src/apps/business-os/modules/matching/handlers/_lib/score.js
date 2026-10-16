// Matching-Scoring, treu portiert aus importer.rs (4794af03d) :3695-4015
// (matching_requirement_text, matching_object_text, push_value,
// build_match_items, dimension_score, content_overlap_score, salient_terms,
// is_match_stopword, snippet_for_dimension, explanation_for_score,
// total_match_score). Reine Funktionen.
import { cleanText, lines, truncateChars } from './text.js';

// importer.rs::push_value
function pushValue(parts, label, value) {
  if (value === undefined) return;
  let text;
  if (typeof value === 'string') text = value;
  else if (Array.isArray(value)) text = value.filter((v) => typeof v === 'string').join(', ');
  else if (typeof value === 'boolean') text = String(value);
  else if (typeof value === 'number') text = rustNumberString(value);
  else text = '';
  text = cleanText(text);
  if (text !== '') parts.push(`${label}: ${text}`);
}

// serde_json::Number::to_string – ganze Zahlen ohne ".0" (i64/u64), Gleitkomma
// über ryu. UNKLAR: JSON-Gleitkomma mit ganzzahligem Wert ("3.0") ist in JS
// nicht von 3 unterscheidbar; Rust schriebe "3.0".
function rustNumberString(n) {
  return String(n);
}

export function matchingRequirementText(r) {
  const parts = [];
  pushValue(parts, 'Titel', r.title);
  pushValue(parts, 'Quelle', r.sourceName);
  pushValue(parts, 'Ort', r.location);
  pushValue(parts, 'Rolle', r.aboutRole);
  pushValue(parts, 'Anforderungen', r.objectRequirements);
  pushValue(parts, 'Aufgaben', r.responsibilities);
  pushValue(parts, 'Benefits', r.benefits);
  pushValue(parts, 'Rohtext', r.rawText);
  return parts.join('\n');
}

export function matchingObjectText(o) {
  const parts = [];
  pushValue(parts, 'Name', o.name);
  pushValue(parts, 'Aktuelle Rolle', o.currentRole);
  pushValue(parts, 'Ziel', o.desiredPosition);
  pushValue(parts, 'Abschluss', o.highestDegree);
  pushValue(parts, 'Region', o.region);
  pushValue(parts, 'Sprachen', o.languages);
  pushValue(parts, 'Skills', o.skills);
  pushValue(parts, 'Zusammenfassung', o.executiveInfo);
  pushValue(parts, 'Rohtext', o.rawText);
  return parts.join('\n');
}

const DIMENSIONS = [
  ['REQ-1', 'Fachliche Qualifikation', 'skill', 'base'],
  ['REQ-2', 'Berufserfahrung', 'experience', 'performance'],
  ['REQ-3', 'Ausbildung / Abschluss', 'education', 'base'],
  ['REQ-4', 'Sprache und Kommunikation', 'language', 'performance'],
  ['REQ-5', 'Motivation und Arbeitsweise', 'other', 'enthusiasm'],
];

const KEYWORDS = {
  skill: ['gebäudeautomation', 'automatisierung', 'cad', 'catia', 'sap', 'test', 'validierung', 'python', 'sql', 'elektro', 'maschinenbau'],
  experience: ['erfahrung', 'projekt', 'service', 'sonderanlagenbau', 'automotive', 'technik', 'kunden', 'leitung'],
  education: ['studium', 'bachelor', 'master', 'techniker', 'ausbildung', 'ingenieur', 'maschinenbau', 'elektrotechnik'],
  language: ['deutsch', 'englisch', 'kommunikation', 'international'],
  other: ['team', 'selbstständig', 'flexibel', 'qualität', 'kundenorientiert', 'motivation'],
};

const clamp = (v, lo, hi) => Math.min(Math.max(v, lo), hi);

const STOPWORDS = new Set([
  'und', 'oder', 'der', 'die', 'das', 'den', 'dem', 'des', 'ein', 'eine', 'einen', 'einem',
  'einer', 'eines', 'mit', 'für', 'von', 'vom', 'zur', 'zum', 'auf', 'aus', 'bei', 'nach',
  'über', 'unter', 'sowie', 'sind', 'wird', 'werden', 'haben', 'sich', 'auch', 'nicht',
  'dass', 'ihre', 'ihrer', 'unsere', 'unser', 'diese', 'dieser', 'dieses', 'sowohl', 'and',
  'the', 'for', 'with', 'from', 'that', 'this', 'your', 'you', 'our', 'are', 'will', 'have',
  'should', 'must', 'they', 'their', 'into', 'such', 'able', 'well',
]);

// Rust char::is_alphanumeric = Alphabetic ∪ Numeric (Nd, Nl, No).
export function salientTerms(text) {
  const out = new Set();
  for (const word of text.split(/[^\p{Alphabetic}\p{N}]/u)) {
    const lowered = word.toLowerCase();
    if ([...lowered].length >= 4 && !STOPWORDS.has(lowered)) out.add(lowered);
  }
  return out;
}

export function contentOverlapScore(jobText, cvText) {
  const jobTerms = salientTerms(jobText);
  if (jobTerms.size === 0) return 0.1;
  const cvTerms = salientTerms(cvText);
  let overlap = 0;
  for (const term of jobTerms) if (cvTerms.has(term)) overlap += 1;
  return clamp(0.1 + (overlap / jobTerms.size) * 0.8, 0, 0.96);
}

export function dimensionScore(dimension, jobText, cvText) {
  const keywords = KEYWORDS[dimension] ?? KEYWORDS.other;
  const jobLower = jobText.toLowerCase();
  const cvLower = cvText.toLowerCase();
  const required = keywords.filter((k) => jobLower.includes(k.toLowerCase()));
  if (required.length === 0) return contentOverlapScore(jobText, cvText);
  const hits = required.filter((k) => cvLower.includes(k.toLowerCase())).length;
  return clamp(0.28 + (hits / required.length) * 0.68, 0, 0.96);
}

const NEEDLES = {
  skill: ['Skills', 'Anforderungen', 'Kompetenz', 'Kenntnisse'],
  experience: ['Erfahrung', 'Berufserfahrung', 'Rolle', 'Projekt'],
  education: ['Studium', 'Bachelor', 'Master', 'Ausbildung', 'Abschluss'],
  language: ['Deutsch', 'Englisch', 'Sprache', 'Kommunikation'],
  other: ['Team', 'selbstständig', 'flexibel', 'Qualität', 'Arbeitsweise'],
};

export function snippetForDimension(text, dimension) {
  const needles = NEEDLES[dimension] ?? NEEDLES.other;
  for (const line of lines(text)) {
    const lower = line.toLowerCase();
    if (needles.some((n) => lower.includes(n.toLowerCase()))) return truncateChars(line, 240);
  }
  return truncateChars(text, 240);
}

export function explanationForScore(title, score) {
  if (score >= 0.78) return `${title}: starke Übereinstimmung zwischen Anforderung und CV-Evidenz.`;
  if (score >= 0.42) return `${title}: teilweise passende Evidenz vorhanden; Details sollten geprüft werden.`;
  return `${title}: aktuell nur schwache oder indirekte Evidenz im CV.`;
}

// Rust f64::round – halbe Werte von null weg.
const roundHalfAway = (v) => Math.sign(v) * Math.round(Math.abs(v));

export function buildMatchItems(companyId, jobId, candidateId, jobText, cvText, nowIso) {
  return DIMENSIONS.map(([reqId, title, dimension, priority]) => {
    const score = dimensionScore(dimension, jobText, cvText);
    const matchLevel = score >= 0.78 ? 'full' : score >= 0.42 ? 'partial' : 'none';
    return {
      id: `${companyId}|${jobId}|${candidateId}|${reqId}`,
      companyId,
      jobId,
      candidateId,
      requirementId: reqId,
      title,
      dimension,
      priority,
      matchLevel,
      matchScore: score,
      jobSnippet: snippetForDimension(jobText, dimension),
      cvSnippet: snippetForDimension(cvText, dimension),
      explanation: explanationForScore(title, score),
      createdAt: nowIso,
      updatedAt: nowIso,
      priorityKey: priority === 'base' ? 2 : priority === 'performance' ? 1 : 0,
      matchLevelKey: matchLevel === 'full' ? 2 : matchLevel === 'partial' ? 1 : 0,
      matchScoreKey: roundHalfAway(score * 100),
    };
  });
}

export function totalMatchScore(items) {
  if (items.length === 0) return 0;
  const sum = items.map((i) => i.matchScore).filter((v) => typeof v === 'number').reduce((a, b) => a + b, 0);
  return clamp(roundHalfAway((sum / items.length) * 100), 0, 100);
}
