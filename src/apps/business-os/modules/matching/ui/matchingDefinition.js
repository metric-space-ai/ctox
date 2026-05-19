export const DEFAULT_MATCHING_DEFINITION = Object.freeze({
  id: 'candidate_job.v1',
  title: 'Candidate-Job Matching',
  description: 'Default demo setup for matching candidates against job requirements.',
  engine: {
    version: 'generic_matching.v1',
    sourceRole: 'source',
    objectRole: 'object',
    resultRole: 'match'
  },
  labels: {
    requirementsColumn: 'Anforderungen',
    matchesColumn: 'Matches',
    objectsColumn: 'Objekte',
    sourceGroup: 'Quelle',
    sourceGroupPlural: 'Quellen',
    sourceRecord: 'Anforderung',
    sourceRecordPlural: 'Anforderungen',
    sourceDrawerTitle: 'Stelle',
    objectRecord: 'Objekt',
    objectRecordPlural: 'Objekte',
    objectDrawerTitle: 'CV',
    objectActionSubject: 'Kandidat',
    objectViewButton: 'Object',
    matchAction: 'Matchen',
    matchActionTitle: 'Kandidat mit aktiver Anforderung matchen',
    matchActionAria: 'Kandidat matchen',
    matchDetails: 'Match-Details',
    relationTitle: 'Kontaktstatus',
    relationExists: 'Kontakt besteht',
    relationMissing: 'Noch kein Kontakt'
  },
  placeholders: {
    sourceSearch: 'Anforderung, Quellen oder Bereich suchen...',
    matchSearch: 'Match, Kriterium oder Quelle suchen...',
    objectSearch: 'Objekt, Skill, Ort oder Stichwort suchen...'
  },
  drawers: {
    sourceSections: {
      aboutSource: 'Über das Unternehmen',
      role: 'Rolle & Aufgaben',
      requirements: 'Anforderungen',
      benefits: 'Benefits',
      closing: 'Hinweise zur Bewerbung'
    },
    objectSections: {
      profile: 'Profil',
      masterData: 'Stammdaten',
      executiveInfo: 'Executive Info',
      experience: 'Berufserfahrung',
      education: 'Ausbildung',
      skills: 'Fachkenntnisse',
      languages: 'Sprachkenntnisse',
      other: 'Weitere Fähigkeiten'
    }
  },
  prompts: {
    system: 'You are a configurable matching engine. Use the active matching definition and return only the requested JSON.',
    domainSystem: 'You are a matching engine for an HR recruiting application.',
    sourceTextLabel: 'Job description',
    objectTextLabel: 'CV',
    task: 'Compare a job description with a candidate CV and produce structured "match items" that describe how well the candidate fits the requirements from the perspective of a recruiter who offers a candidate to a company.'
  }
});

let activeDefinition = DEFAULT_MATCHING_DEFINITION;

export function getActiveMatchingDefinition() {
  return activeDefinition || DEFAULT_MATCHING_DEFINITION;
}

export function setActiveMatchingDefinition(definition) {
  activeDefinition = normalizeMatchingDefinition(definition);
  return activeDefinition;
}

export function matchingText(path, fallback = '') {
  const value = getByPath(getActiveMatchingDefinition(), path);
  return typeof value === 'string' && value.trim() ? value : fallback;
}

function normalizeMatchingDefinition(definition) {
  if (!definition || typeof definition !== 'object') return DEFAULT_MATCHING_DEFINITION;
  return deepMerge(DEFAULT_MATCHING_DEFINITION, definition);
}

function deepMerge(base, patch) {
  if (!patch || typeof patch !== 'object' || Array.isArray(patch)) return base;
  const next = { ...base };
  for (const [key, value] of Object.entries(patch)) {
    if (value && typeof value === 'object' && !Array.isArray(value) && base[key] && typeof base[key] === 'object') {
      next[key] = deepMerge(base[key], value);
    } else {
      next[key] = value;
    }
  }
  return next;
}

function getByPath(obj, path) {
  return String(path || '')
    .split('.')
    .filter(Boolean)
    .reduce((acc, key) => (acc == null ? acc : acc[key]), obj);
}
