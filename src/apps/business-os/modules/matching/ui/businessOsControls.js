import {
  bindUniversalImportSourceSwitching,
  buildUniversalImportCommandPayloadFromDrawer,
  renderUniversalImportDrawerMarkup
} from '../../../shared/universal-importer.js';
import { getActiveMatchingDefinition } from './matchingDefinition.js';

const THEME_KEY = 'ctox.businessOs.requirementMatching.theme';
const LANG_KEY = 'ctox.businessOs.requirementMatching.lang';
const COLUMN_SETTINGS_KEY = 'ctox.businessOs.requirementMatching.columnSettings';
const SEARCH_SORT_SETTINGS_KEY = 'ctox.businessOs.requirementMatching.searchSortSettings';
const TRANSLATION_SETTINGS_KEY = 'ctox.businessOs.requirementMatching.translations';
const IMPORT_STATUS_KEY = 'ctox.businessOs.matching.importStatus';
const COMMAND_TIMEOUT_MS = 8000;
const SUPPORTED_LANGUAGES = ['de', 'en'];
const pendingTranslationRequests = new Set();
const MATCHING_DEFINITION = getActiveMatchingDefinition();
const MATCHING_LABELS = MATCHING_DEFINITION.labels || {};
const MATCHING_DEFINITION_ID = MATCHING_DEFINITION.id || 'generic_matching.v1';
const MATCHING_SCHEMA_VERSION = MATCHING_DEFINITION.engine?.version || 'generic_matching.v1';

const dictionary = new Map([
  ['Quellen', 'Sources'],
  ['UNTERNEHMEN', 'COMPANIES'],
  ['Anforderungen', 'Requirements'],
  ['AUSSCHREIBUNGEN', 'REQUIREMENT REQUIREMENT_SOURCES'],
  ['Objekte', 'Objects'],
  ['KANDIDATEN', 'OBJECTS'],
  ['Liste', 'List'],
  ['Tabelle', 'Table'],
  ['Alle Standorte', 'All locations'],
  ['Keine Quellen', 'No sources'],
  ['Keine Quellen in der Datenbank gefunden.', 'No sources found in the database.'],
  ['Keine Objekte in der Datenbank gefunden.', 'No objects found in the database.'],
  ['Sortieren: Bester Match', 'Sort: Best match'],
  ['Sortieren: Neueste zuerst', 'Sort: Newest first'],
  ['Sortieren: Älteste zuerst', 'Sort: Oldest first'],
  ['Sortieren: Name', 'Sort: Name'],
  ['Sortieren: Taxonomie', 'Sort: Taxonomy'],
  ['PDF hinzufügen', 'Add PDF'],
  ['Notizen', 'Notes'],
  ['Fortschritt', 'Progress'],
  ['Kontaktstatus', 'Contact status'],
  ['Kontakt besteht', 'Contact exists'],
  ['Noch kein Kontakt', 'No contact yet'],
  ['Match-Details', 'Match details'],
  ['Anforderung', 'Requirement'],
  ['Schließen', 'Close'],
  ['Details schließen', 'Close details']
]);

const placeholderDictionary = new Map([
  ['Quellen, Standort oder Anforderung suchen…', 'Search source, requirement, or area...'],
  ['Requirement, Quelle, Ort oder Stichwort suchen…', 'Search requirement, source, attribute, or keyword...'],
  ['Objekt, Skill, Ort oder Stichwort suchen…', 'Search object, attribute, source, or keyword...'],
  ['Notizen zum Matchingprozess…', 'Notes on the matching process...']
]);

const bundledTranslations = {
  en: {
    text: dictionary,
    placeholder: placeholderDictionary,
    title: dictionary
  }
};

const COLUMN_DEFAULTS = {
  requirements: {
    label: MATCHING_LABELS.requirementsColumn || 'Anforderungen',
    singular: MATCHING_LABELS.sourceRecord || 'Anforderung',
    plural: MATCHING_LABELS.sourceRecordPlural || 'Anforderungen',
    entityType: 'requirement'
  },
  matches: {
    label: 'Matches',
    singular: 'Match',
    plural: 'Matches',
    entityType: 'match'
  },
  objects: {
    label: MATCHING_LABELS.objectsColumn || 'Objekte',
    singular: MATCHING_LABELS.objectRecord || 'Objekt',
    plural: MATCHING_LABELS.objectRecordPlural || 'Objekte',
    entityType: 'object'
  }
};

const COLUMN_PROMPTS = {
  requirements: {
    parser: 'ctox.requirement.sources',
    structure: 'matchingRequirement.v1',
    storage: {
      collection: 'business_records',
      definitionCollection: 'business_definitions',
      moduleId: 'matching',
      definitionId: `${MATCHING_DEFINITION_ID}.sources`,
      entityType: 'requirement',
      canonicalField: 'data',
      schemaVersion: `${MATCHING_SCHEMA_VERSION}.source`,
      recordKey: 'data.requirement.id || data.requirementSource.externalRef || generated',
      indexes: {
        indexText: ['data.requirement.title', 'data.source.name', 'data.requirement.location', 'data.requirement.objectRequirements', 'data.requirementSource.rawText'],
        sortKey: 'data.requirement.updatedAt || data.requirement.title',
        statusKey: 'data.requirement.status',
        scoreKey: 'data.requirementSource.parsed.urgencyValue'
      },
      compatibilityProjection: ['sources', 'requirements', 'requirementSources']
    },
    prompt: `Du erhältst eine oder mehrere Quellen zu einer Anforderung: PDF, ZIP, Excel, CSV, URL, HTML oder Freitext.

Erzeuge daraus ein strukturiertes Anforderungsobjekt für das Requirement-Matching-Modell.

Regeln:
- Antworte nur mit einem einzelnen gültigen JSON-Objekt.
- Keine Markdown-Blöcke, keine Erklärungen außerhalb des JSON.
- Extrahiere Aufgaben, Anforderungen, Benefits, Standort, Arbeitsmodell, Sprache, Gehalt und Metadaten.
- Gib Aufgaben und Anforderungen zusätzlich als Arrays aus, damit die Drawer nicht als Fließtextwände gerendert werden.
- Bewerte Agency Type, Incentives, Urgency, Relax, Vacancy Age und Fachlevel jeweils mit Value 0..3 und Evidence-Array.
- Wenn Werte nicht sicher ableitbar sind, setze sie auf null oder leere Strings/Arrays, nicht halluzinieren.
- Sprache: verwende die Sprache der Quelle.`,
    schema: {
      requirement: {
        title: 'string',
        sourceId: 'string',
        status: 'open | on-hold | closed',
        workModel: 'Vollzeit | Teilzeit | Schicht | Werkstudent | Praktikum | null',
        remote: 'boolean | null',
        remotePercent: 'integer 0..100 | null',
        salaryMin: 'number | null',
        salaryMax: 'number | null',
        salaryPeriod: 'year | month | hour | null',
        salaryCurrency: 'EUR | string | null',
        aboutSource: 'string',
        aboutRole: 'string',
        objectRequirements: 'string',
        responsibilities: ['string'],
        requirements: ['string'],
        benefits: ['string'],
        closingNotes: 'string',
        language: 'de | en | other'
      },
      requirementSource: {
        source: 'BA | StepStone | Indeed | LinkedIn | Other',
        sourceUrl: 'string | null',
        externalRef: 'string | null',
        rawText: 'string',
        parsed: {
          aboutSource: 'string',
          aboutRole: 'string',
          objectRequirements: 'string',
          responsibilities: ['string'],
          requirements: ['string'],
          benefits: ['string'],
          closingNotes: 'string',
          agencyTypeValue: 'integer 0..3',
          agencyTypeEvidence: ['string'],
          incentivesValue: 'integer 0..3',
          incentivesEvidence: ['string'],
          urgencyValue: 'integer 0..3',
          urgencyEvidence: ['string'],
          relaxValue: 'integer 0..3',
          relaxEvidence: ['string'],
          vacancyAgeClass: 'integer 0..3',
          vacancyAgeEvidence: ['string'],
          fachlevelClass: 'integer 0..3',
          fachlevelEvidence: ['string'],
          kldbCode: 'string',
          kldbEvidence: ['string'],
          workModel: 'Vollzeit | Teilzeit | Schicht | Werkstudent | Praktikum | null',
          remote: 'boolean | null',
          remotePercent: 'integer 0..100 | null',
          relocation: 'boolean | null',
          visaSponsorship: 'boolean | null'
        },
        parsingMeta: {
          schemaVersion: 'matching.requirement.v1',
          confidence: 'number 0..1'
        }
      }
    },
    display: {
      version: 'ctox.business-os.display.v1',
      collection: 'requirements',
      sourceCollections: ['sources', 'requirements', 'requirementSources'],
      primaryKey: 'requirement.id',
      grouping: {
        field: 'source.name',
        label: 'Quellen',
        fallback: 'Ohne Quellen'
      },
      list: {
        title: 'requirement.title',
        subtitle: ['source.name', 'requirement.location', 'requirement.workModel'],
        meta: ['requirement.status', 'requirementSource.source', 'requirementSource.parsingMeta.confidence'],
        badges: [
          { field: 'requirement.fachlevelClass', label: 'Level', map: 'fachlevelClass' },
          { field: 'requirement.remotePercent', label: 'Remote', suffix: '%' },
          { field: 'requirementSource.parsed.urgencyValue', label: 'Dringlichkeit', scale: [0, 3] }
        ],
        body: [
          { label: 'Aufgabe', field: 'requirement.aboutRole', maxLines: 3 },
          { label: 'Anforderungen', field: 'requirement.objectRequirements', maxLines: 4 },
          { label: 'Benefits', field: 'requirement.benefits', type: 'chips', maxItems: 6 }
        ]
      },
      search: {
        placeholder: 'Quellen, Standort oder Anforderung suchen...',
        fields: ['source.name', 'requirement.title', 'requirement.location', 'requirement.aboutRole', 'requirement.objectRequirements', 'requirementSource.rawText']
      },
      sort: [
        { id: 'updated_desc', label: 'Neueste zuerst', field: 'requirement.updatedAt', direction: 'desc' },
        { id: 'source_asc', label: 'Quellen', field: 'source.name', direction: 'asc' },
        { id: 'urgency_desc', label: 'Dringlichkeit', field: 'requirementSource.parsed.urgencyValue', direction: 'desc' }
      ],
      detail: {
        drawer: 'left',
        sections: [
          { title: 'Rolle', fields: ['requirement.aboutRole', 'requirement.objectRequirements'] },
          { title: 'Parsing Evidence', fields: ['requirementSource.parsed.kldbEvidence', 'requirementSource.parsed.fachlevelEvidence'] }
        ]
      }
    }
  },
  objects: {
    parser: 'ctox.profile.sources',
    structure: 'matchingObject.v1',
    storage: {
      collection: 'business_records',
      definitionCollection: 'business_definitions',
      moduleId: 'matching',
      definitionId: `${MATCHING_DEFINITION_ID}.objects`,
      entityType: 'object',
      canonicalField: 'data',
      schemaVersion: `${MATCHING_SCHEMA_VERSION}.object`,
      recordKey: 'data.object.id || data.object.email || generated',
      indexes: {
        indexText: ['data.object.name', 'data.object.currentRole', 'data.object.desiredPosition', 'data.object.skills', 'data.object.region'],
        sortKey: 'data.object.name',
        statusKey: 'data.object.objectStatus',
        scoreKey: 'activeMatch.score || -1'
      },
      compatibilityProjection: ['objects']
    },
    prompt: `Du erhältst einen Objektquelle oder Objektequellen: PDF, ZIP, URL, LinkedIn, Markdown, Freitext oder strukturierte Daten.

Erzeuge daraus ein Objekteprofil im Requirement-Matching-Modell.

Regeln:
- Antworte nur mit einem einzelnen gültigen JSON-Objekt.
- Keine Markdown-Blöcke, keine Erklärungen außerhalb des JSON.
- Extrahiere Stammdaten, Kontakt, aktuelle Rolle, Zielrolle, Skills, Sprachen, Ausbildung, Berufserfahrung, Wünsche und Verfügbarkeit.
- Leite executiveInfo aus dem Object ab: fachliche Qualifikation, Methodenkompetenz, Leadership-Fähigkeit, Gehaltswunsch und Ort.
- Gib Ausbildung und Berufserfahrung als strukturierte Arrays aus. Nutze documents/additional nur für Rohdokumente oder zusätzliche Nachweise.
- Wenn ein Feld nicht sicher belegbar ist, setze null oder leere Arrays. Keine erfundenen Informationen.`,
    schema: {
      object: {
        name: 'string',
        firstName: 'string | null',
        lastName: 'string | null',
        birthDate: 'date | null',
        nationality: 'string | null',
        gender: 'divers | weiblich | männlich | keine Angabe | null',
        email: 'string | null',
        phone: 'string | null',
        address: {
          street: 'string | null',
          postalCode: 'string | null',
          city: 'string | null',
          country: 'string'
        },
        currentRole: 'string | null',
        desiredPosition: 'string | null',
        taxonomy: 'string | null',
        industry: 'string | null',
        availabilityFrom: 'date | null',
        region: 'string | null',
        travelOk: 'boolean | null',
        workModelWish: 'Vollzeit | Teilzeit | flexibel | null',
        highestDegree: 'string | null',
        degree: 'string | null',
        education: [{
          degree: 'string | null',
          major: 'string | null',
          institution: 'string | null',
          location: 'string | null',
          start_date: 'date|string|null',
          end_date: 'date|string|null',
          details: ['string']
        }],
        experience: [{
          job_title: 'string | null',
          employer: 'string | null',
          location: 'string | null',
          start_date: 'date|string|null',
          end_date: 'date|string|null',
          job_description: ['string']
        }],
        languages: [{ code: 'string', level: 'string' }],
        skills: ['string'],
        softSkills: ['string'],
        executiveInfo: {
          fachlicheQualifikation: 'string | null',
          methodenKompetenz: 'string | null',
          leadershipFaehigkeit: 'string | null',
          gehaltswunschUndOrt: 'string | null'
        },
        documents: [{
          kind: 'Object | Zeugnis | Zertifikat | Nachweis | Foto | Sonstiges',
          filename: 'string',
          parsed: 'boolean',
          meta: 'object'
        }],
        objectStatus: 'neu | in_klärung | aktiv | inaktiv | gesperrt'
      }
    },
    display: {
      version: 'ctox.business-os.display.v1',
      collection: 'objects',
      sourceCollections: ['objects', 'object_documents', 'object_photo_chunks'],
      primaryKey: 'object.id',
      list: {
        title: 'object.name',
        subtitle: ['object.currentRole', 'object.region', 'object.availabilityFrom'],
        avatar: {
          imageField: 'object.photo',
          fallback: 'initials(object.name)'
        },
        meta: ['object.objectStatus', 'object.taxonomy', 'object.workModelWish'],
        badges: [
          { field: 'object.skills', label: 'Skills', type: 'chips', maxItems: 5 },
          { field: 'object.languages', label: 'Sprachen', type: 'chips', maxItems: 3 },
          { field: 'object.travelOk', label: 'Reise', type: 'boolean' }
        ],
        body: [
          { label: 'Fachlich', field: 'object.executiveInfo.fachlicheQualifikation', maxLines: 3 },
          { label: 'Methoden', field: 'object.executiveInfo.methodenKompetenz', maxLines: 2 },
          { label: 'Leadership', field: 'object.executiveInfo.leadershipFaehigkeit', maxLines: 2 }
        ]
      },
      search: {
        placeholder: 'Objekt, Skill, Ort oder Stichwort suchen...',
        fields: ['object.name', 'object.currentRole', 'object.desiredPosition', 'object.skills', 'object.region', 'object.documents.meta.rawText']
      },
      sort: [
        { id: 'best_match', label: 'Bester Match', field: 'match.score', direction: 'desc', requiresSelection: 'requirement' },
        { id: 'updated_desc', label: 'Neueste zuerst', field: 'object.updatedAt', direction: 'desc' },
        { id: 'name_asc', label: 'Name', field: 'object.name', direction: 'asc' }
      ],
      detail: {
        drawer: 'right',
        sections: [
          { title: 'Profil', fields: ['object.currentRole', 'object.desiredPosition', 'object.executiveInfo'] },
          { title: 'Object', fields: ['object.documents', 'object.skills', 'object.languages'] }
        ]
      }
    }
  },
  matches: {
    parser: 'ctox.match.scoring',
    structure: 'matchingResult.items.v1',
    storage: {
      collection: 'business_records',
      definitionCollection: 'business_definitions',
      moduleId: 'matching',
      definitionId: `${MATCHING_DEFINITION_ID}.matches`,
      entityType: 'match',
      canonicalField: 'data',
      schemaVersion: `${MATCHING_SCHEMA_VERSION}.match`,
      recordKey: 'data.match.sourceId + \"|\" + data.match.requirementId + \"|\" + data.match.objectId',
      indexes: {
        indexText: ['data.source.name', 'data.requirement.title', 'data.object.name', 'data.match.items.title', 'data.match.items.explanation'],
        sortKey: 'data.match.updatedAt || data.match.score',
        statusKey: 'data.match.status',
        scoreKey: 'data.match.score'
      },
      compatibilityProjection: ['matches']
    },
    prompt: `You are a matching engine for an HR recruiting application.

Your task:
Compare a job description with a candidate CV and produce structured "match items" that describe how well the candidate fits the requirements from the perspective of a recruiter who offers a candidate to a company.

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

- No markdown, no extra text.`,
    schema: {
      match: {
        sourceId: 'string',
        requirementId: 'string',
        objectId: 'string',
        status: 'prospecting | prematch | active | interview | offer | hired | rejected | on-hold',
        progress: 'integer 0..100',
        score: 'integer 0..100',
        items: [{
          requirementId: 'string',
          title: 'string',
          dimension: 'education | experience | skill | language | other',
          priority: 'base | performance | enthusiasm',
          matchLevel: 'full | partial | none',
          matchScore: 'number 0..1',
          jobSnippet: 'string',
          cvSnippet: 'string',
          explanation: 'string'
        }]
      }
    },
    display: {
      version: 'ctox.business-os.display.v1',
      collection: 'matches',
      sourceCollections: ['matches', 'requirements', 'requirementSources', 'objects', 'sources'],
      primaryKey: 'match.id',
      matrix: {
        rows: {
          collection: 'requirements',
          key: 'requirement.id',
          label: 'requirement.title',
          groupBy: 'source.name'
        },
        columns: {
          collection: 'objects',
          key: 'object.id',
          label: 'object.name',
          subtitle: 'object.taxonomy'
        },
        cell: {
          value: 'match.score',
          colorScale: 'scoreBucket(match.score)',
          strikeWhen: 'hasConflict(match.items)',
          iconsFrom: 'conflictTypes(match.items)'
        }
      },
      buckets: [
        { id: 'base', label: 'Basis', itemsWhere: { priority: 'base' } },
        { id: 'performance', label: 'Leistung', itemsWhere: { priority: 'performance' } },
        { id: 'enthusiasm', label: 'Begeisterung', itemsWhere: { priority: 'enthusiasm' } }
      ],
      itemCard: {
        title: 'item.title',
        score: 'item.matchScore',
        subtitle: 'item.dimension',
        snippets: [
          { label: 'Stellenausschreibung', field: 'item.jobSnippet' },
          { label: 'CV', field: 'item.cvSnippet' }
        ],
        explanation: 'item.explanation',
        conflictWhen: 'isKnownConflictTitle(item.title)'
      },
      search: {
        placeholder: 'Match, Anforderung, Objekt oder Kriterium suchen...',
        fields: ['source.name', 'requirement.title', 'object.name', 'match.items.title', 'match.items.explanation']
      },
      sort: [
        { id: 'score_desc', label: 'Bester Match', field: 'match.score', direction: 'desc' },
        { id: 'updated_desc', label: 'Neueste zuerst', field: 'match.updatedAt', direction: 'desc' },
        { id: 'conflicts_desc', label: 'Konflikte zuerst', expression: 'countConflicts(match.items)', direction: 'desc' }
      ],
      detail: {
        drawer: 'bottom',
        sections: [
          { title: 'Score', fields: ['match.score', 'match.status', 'match.progress'] },
          { title: 'Nachweise', component: 'bucketedMatchItems', source: 'match.items' }
        ]
      }
    }
  }
};

initTheme();
initLanguage();
initColumnLabels();
initColumnDrawers();
initContextMenu();
renderImportStatuses();

function getMatchingRoot() {
  return document.querySelector('[data-matching-module="native"]') || document.body;
}

function queryMatchingAll(selector) {
  return getMatchingRoot().querySelectorAll(selector);
}

function initTheme() {
  const params = new URLSearchParams(window.location.search);
  const requested = params.get('theme');
  const saved = requested === 'light' || requested === 'dark'
    ? requested
    : document.documentElement.dataset.theme || localStorage.getItem(THEME_KEY) || 'system';
  applyTheme(saved);
  for (const button of queryMatchingAll('[data-theme-choice]')) {
    button.addEventListener('click', () => {
      const theme = button.dataset.themeChoice || 'system';
      localStorage.setItem(THEME_KEY, theme);
      applyTheme(theme);
    });
  }
  window.addEventListener('message', (event) => {
    if (event.data?.type !== 'ctox-business-os-preferences') return;
    applyTheme(event.data.theme);
  });
}

function applyTheme(theme) {
  const value = theme === 'light' || theme === 'dark' ? theme : 'system';
  const root = getMatchingRoot();
  if (value === 'system') {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = value;
  }
  for (const button of queryMatchingAll('[data-theme-choice]')) {
    button.setAttribute('aria-pressed', String(button.dataset.themeChoice === value));
  }
}

function initLanguage() {
  const params = new URLSearchParams(window.location.search);
  const requestedRaw = String(params.get('lang') || '').toLowerCase();
  const requested = SUPPORTED_LANGUAGES.includes(requestedRaw) ? requestedRaw : '';
  if (requested) localStorage.setItem(LANG_KEY, requested);
  const saved = requested || document.documentElement.lang || localStorage.getItem(LANG_KEY) || 'de';
  applyLanguage(saved, { reloadForGerman: false });
  for (const button of queryMatchingAll('[data-lang-choice]')) {
    button.addEventListener('click', () => {
      const lang = button.dataset.langChoice || 'de';
      localStorage.setItem(LANG_KEY, lang);
      applyLanguage(lang, { reloadForGerman: true });
    });
  }
  window.addEventListener('message', (event) => {
    if (event.data?.type !== 'ctox-business-os-preferences') return;
    applyLanguage(event.data.language, { reloadForGerman: false });
  });
}

function applyLanguage(lang, { reloadForGerman = false } = {}) {
  const value = normalizeLanguage(lang);
  const root = getMatchingRoot();
  root.dataset.lang = value;
  for (const button of queryMatchingAll('[data-lang-choice]')) {
    button.setAttribute('aria-pressed', String(button.dataset.langChoice === value));
  }
  if (value !== 'de') translateDocument(root);
}

function normalizeLanguage(lang) {
  const value = String(lang || 'de').toLowerCase();
  return SUPPORTED_LANGUAGES.includes(value) ? value : value || 'de';
}

function translateDocument(root = getMatchingRoot()) {
  translateTextNodes(root);
  for (const option of root.querySelectorAll('option')) {
    translateElementText(option);
  }
  for (const el of root.querySelectorAll('input[placeholder], textarea[placeholder]')) {
    const next = translatePhrase(el.getAttribute('placeholder'), 'placeholder');
    if (next) el.setAttribute('placeholder', next);
  }
  for (const el of root.querySelectorAll('[title]')) {
    const next = translatePhrase(el.getAttribute('title'), 'title');
    if (next) el.setAttribute('title', next);
  }
}

function translateTextNodes(root) {
  for (const node of collectTextNodes(root)) translateNodeText(node);
}

function collectTextNodes(root) {
  const nodes = [];
  const visit = (node) => {
    if (!node) return;
    if (node.nodeType === Node.TEXT_NODE) {
      nodes.push(node);
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const tagName = node.tagName?.toLowerCase();
    if (tagName === 'script' || tagName === 'style' || tagName === 'textarea' || tagName === 'input') return;
    for (const child of node.childNodes) visit(child);
  };
  visit(root);
  return nodes;
}

function translateElementText(el) {
  const translated = translatePhrase(el.textContent.trim(), 'text');
  if (translated) el.textContent = translated;
}

function translateNodeText(node) {
  const raw = node.nodeValue;
  const trimmed = raw.trim();
  const translated = translatePhrase(trimmed, 'text');
  if (translated) node.nodeValue = raw.replace(trimmed, translated);
}

function translatePhrase(value, kind = 'text') {
  const raw = String(value || '');
  const trimmed = raw.trim();
  if (!trimmed) return '';
  const lang = normalizeLanguage(localStorage.getItem(LANG_KEY) || 'de');
  if (lang === 'de') return trimmed;
  const runtime = readRuntimeTranslations();
  const runtimeValue = runtime?.[lang]?.[kind]?.[trimmed];
  if (typeof runtimeValue === 'string' && runtimeValue.trim()) return runtimeValue.trim();
  const bundled = bundledTranslations[lang]?.[kind]?.get(trimmed);
  if (bundled) return bundled;
  requestMissingTranslation({ lang, kind, text: trimmed });
  return trimmed;
}

function readRuntimeTranslations() {
  try {
    const parsed = JSON.parse(localStorage.getItem(TRANSLATION_SETTINGS_KEY) || '{}');
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function requestMissingTranslation({ lang, kind, text }) {
  const key = `${lang}:${kind}:${text}`;
  if (pendingTranslationRequests.has(key)) return;
  pendingTranslationRequests.add(key);
  dispatchCtoxCommand({
    module: 'matching',
    type: 'business_os.i18n.translate',
    record_id: key.slice(0, 128),
    payload: {
      lang,
      kind,
      text,
      storage_key: TRANSLATION_SETTINGS_KEY,
      expected_shape: {
        [lang]: {
          [kind]: {
            [text]: 'translated text'
          }
        }
      }
    },
    client_context: {
      action: 'missing-translation',
      lang,
      kind
    }
  }, { timeoutMs: 1000 }).catch(() => {});
}

function dispatchCtoxCommand(command, { timeoutMs = COMMAND_TIMEOUT_MS } = {}) {
  const requestId = `ctox_cmd_${Date.now()}_${Math.random().toString(16).slice(2)}`;
  return new Promise((resolve) => {
    let done = false;
    const timer = setTimeout(() => {
      if (done) return;
      done = true;
      window.removeEventListener('message', onMessage);
      resolve({ ok: false, status: 'timeout', requestId });
    }, timeoutMs);

    function onMessage(event) {
      if (event.data?.type !== 'ctox-business-os-command-result') return;
      if (event.data.requestId !== requestId) return;
      done = true;
      clearTimeout(timer);
      window.removeEventListener('message', onMessage);
      resolve(event.data);
    }

    window.addEventListener('message', onMessage);
    parent.postMessage({
      type: 'ctox-business-os-command',
      requestId,
      surface: 'matching',
      command
    }, '*');
  });
}

function commandContextFromElement(target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;
  const columnButton = element?.closest?.('[data-column]');
  const panel = element?.closest?.('.panel');
  const field = element?.closest?.('input, textarea, select, button');
  const drawer = element?.closest?.('[data-column-drawer]');
  const importPanel = element?.closest?.('[data-import-panel]');
  const matchItem = element?.closest?.('[data-req], .match-item-card, .matrix-score');

  const column =
    columnButton?.dataset.column ||
    (panel?.id === 'left' ? 'requirements' : panel?.id === 'center' ? 'matches' : panel?.id === 'right' ? 'objects' : '');

  return {
    module: 'matching',
    column,
    entityType: column ? getColumnMeta(column).entityType : '',
    panelId: panel?.id || '',
    drawerSide: drawer?.dataset.columnDrawer || '',
    importSource: importPanel?.dataset.importPanel || '',
    fieldTag: field?.tagName?.toLowerCase() || '',
    fieldId: field?.id || '',
    fieldName: field?.getAttribute?.('name') || '',
    role: matchItem?.getAttribute?.('data-req') ? 'match-item' : '',
    text: element?.textContent?.trim().slice(0, 240) || ''
  };
}

function readColumnSettings() {
  try {
    const parsed = JSON.parse(localStorage.getItem(COLUMN_SETTINGS_KEY) || '{}');
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function readSearchSortSettings() {
  try {
    const parsed = JSON.parse(localStorage.getItem(SEARCH_SORT_SETTINGS_KEY) || '{}');
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function writeSearchSortSettings(settings) {
  localStorage.setItem(SEARCH_SORT_SETTINGS_KEY, JSON.stringify(settings || {}));
}

function updateSearchSortSettings(column, patch) {
  const settings = readSearchSortSettings();
  settings[column] = {
    ...(settings[column] || {}),
    ...Object.fromEntries(
      Object.entries(patch || {}).map(([key, value]) => [key, String(value || '')])
    )
  };
  writeSearchSortSettings(settings);
}

function writeColumnSettings(settings) {
  localStorage.setItem(COLUMN_SETTINGS_KEY, JSON.stringify(settings || {}));
}

function getColumnMeta(column) {
  const defaults = COLUMN_DEFAULTS[column] || COLUMN_DEFAULTS.matches;
  const custom = readColumnSettings()[column] || {};
  return {
    ...defaults,
    ...Object.fromEntries(
      Object.entries(custom).filter(([, value]) => typeof value === 'string' && value.trim())
    )
  };
}

function updateColumnMeta(column, patch) {
  const settings = readColumnSettings();
  const current = settings[column] || {};
  settings[column] = {
    ...current,
    ...Object.fromEntries(
      Object.entries(patch || {}).map(([key, value]) => [key, String(value || '').trim()])
    )
  };
  writeColumnSettings(settings);
  initColumnLabels();
}

function initColumnLabels() {
  const root = getMatchingRoot();
  const selectors = {
    requirements: '#left .column-title',
    matches: '#center .column-title',
    objects: '#right .column-title'
  };

  Object.entries(selectors).forEach(([column, selector]) => {
    const meta = getColumnMeta(column);
    const title = root.querySelector(selector);
    if (title) title.textContent = meta.label;

    root.querySelectorAll(`[data-column="${column}"][data-column-action]`).forEach((button) => {
      const action = button.dataset.columnAction;
      const suffix = action === 'configure'
        ? 'konfigurieren'
        : action === 'import'
          ? 'importieren'
          : action === 'export'
            ? 'exportieren'
            : 'Suche und Sortierung konfigurieren';
      const label = `${meta.plural || meta.label} ${suffix}`;
      button.setAttribute('aria-label', label);
      button.setAttribute('title', label);
    });
  });
}

function initColumnDrawers() {
  const root = getMatchingRoot();
  const backdrop = root.querySelector('[data-column-drawer-backdrop]');
  const drawers = Array.from(root.querySelectorAll('[data-column-drawer]'));
  if (!backdrop || !drawers.length) return;

  document.addEventListener('click', (event) => {
    const actionButton = event.target.closest('[data-column-action]');
    if (!actionButton) return;
    if (!root.contains(actionButton)) return;
    event.preventDefault();
    event.stopPropagation();
    openColumnDrawer(actionButton);
  });

  backdrop.addEventListener('click', closeColumnDrawers);
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') closeColumnDrawers();
  });

  function openColumnDrawer(button) {
    closeColumnDrawers();
    const side = button.dataset.drawerSide || 'right';
    const drawer = root.querySelector(`[data-column-drawer="${side}"]`);
    if (!drawer) return;
    drawer.innerHTML = renderColumnDrawer(button);
    drawer.setAttribute('aria-hidden', 'false');
    drawer.classList.add('is-open');
    backdrop.hidden = false;
    backdrop.classList.add('is-open');
    drawer.querySelector('[data-column-drawer-close]')?.addEventListener('click', closeColumnDrawers);
    bindColumnConfigInputs(drawer, button.dataset.column || 'matches');
    bindSearchSortInputs(drawer, button.dataset.column || 'matches');
    bindImportCommand(drawer, button.dataset.column || 'matches');
    bindUniversalImportSourceSwitching(drawer);
  }

  function closeColumnDrawers() {
    backdrop.hidden = true;
    backdrop.classList.remove('is-open');
    for (const drawer of drawers) {
      drawer.classList.remove('is-open');
      drawer.setAttribute('aria-hidden', 'true');
    }
  }

  function bindColumnConfigInputs(drawer, column) {
    drawer.querySelectorAll('[data-column-config-field]').forEach((input) => {
      input.addEventListener('input', () => {
        updateColumnMeta(column, { [input.dataset.columnConfigField]: input.value });
        const title = drawer.querySelector('[data-column-drawer-title]');
        if (title) {
          const meta = getColumnMeta(column);
          title.textContent = `${meta.label || meta.plural || 'Spalte'} · Konfiguration`;
        }
      });
    });
  }

  function bindSearchSortInputs(drawer, column) {
    drawer.querySelectorAll('[data-search-sort-field]').forEach((input) => {
      input.addEventListener('input', () => {
        updateSearchSortSettings(column, { [input.dataset.searchSortField]: input.value });
      });
    });
    drawer.querySelector('[data-search-sort-save]')?.addEventListener('click', (event) => {
      const patch = {};
      drawer.querySelectorAll('[data-search-sort-field]').forEach((input) => {
        patch[input.dataset.searchSortField] = input.value;
      });
      updateSearchSortSettings(column, patch);
      event.currentTarget.textContent = 'Gespeichert';
    });
  }

  function bindImportCommand(drawer, column) {
    drawer.querySelector('[data-import-run]')?.addEventListener('click', async (event) => {
      const button = event.currentTarget;
      button.textContent = 'Command wird an CTOX übergeben...';
      button.disabled = true;
      let payload = null;
      try {
        payload = await buildImportCommandPayload(drawer, column);
        const commandType = commandTypeForImportColumn(column);
        if (!commandType) {
          throw new Error('Matches werden aus ausgewählter Anforderung und ausgewähltem Objekt erzeugt, nicht als Import.');
        }
        const command = {
          module: 'matching',
          type: commandType,
          record_id: payload.record_id,
          payload,
          client_context: {
            action: 'import',
            column,
            entity_type: payload.entity_type,
            source_type: payload.source_type
          }
        };
        const result = await dispatchCtoxCommand(command);
        recordImportStatus(payload, result);
        renderImportStatuses();
        closeColumnDrawers();
      } catch (error) {
        if (payload) {
          recordImportStatus(payload, { ok: false, status: 'failed', error: String(error?.message || error) });
          renderImportStatuses();
          closeColumnDrawers();
        } else {
          button.textContent = `Import-Command fehlgeschlagen`;
        }
        console.error('[business-os] import command failed', error);
      } finally {
        button.disabled = false;
      }
    });
  }
}

function commandTypeForImportColumn(column) {
  if (column === 'requirements') return 'matching.source.parse_requirement';
  if (column === 'objects') return 'matching.source.parse_object';
  return '';
}

async function buildImportCommandPayload(drawer, column) {
  const meta = getColumnMeta(column);
  const config = COLUMN_PROMPTS[column] || COLUMN_PROMPTS.matches;
  return buildUniversalImportCommandPayloadFromDrawer(drawer, {
    moduleId: 'matching',
    column,
    entityType: meta.entityType,
    recordLabel: meta.singular || meta.label || column,
    title: `${meta.singular || meta.label || column} Import`,
    parser: config.parser,
    definition: {
      schema: config.schema,
      storage: {
        ...(config.storage || {}),
        entityType: meta.entityType,
        labels: {
          label: meta.label,
          singular: meta.singular,
          plural: meta.plural
        }
      },
      display: config.display,
      prompt: config.prompt
    }
  });
}

function readImportStatuses() {
  try {
    const parsed = JSON.parse(localStorage.getItem(IMPORT_STATUS_KEY) || '[]');
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isVisibleImportStatus);
  } catch {
    return [];
  }
}

function writeImportStatuses(items) {
  localStorage.setItem(IMPORT_STATUS_KEY, JSON.stringify((items || []).filter(isVisibleImportStatus).slice(0, 30)));
}

function isVisibleImportStatus(item) {
  if (!item || typeof item !== 'object') return false;
  if (!['requirements', 'objects', 'matches'].includes(String(item.column || ''))) return false;
  const state = normalizeTaskState(item.taskStatus || item.status || item.state);
  if (state === 'completed' || state === 'failed') return false;
  const createdAt = Date.parse(item.createdAt || '');
  if (Number.isFinite(createdAt) && Date.now() - createdAt > 2 * 60 * 60 * 1000) return false;
  return true;
}

function recordImportStatus(payload, commandResult) {
  const result = commandResult?.result || {};
  const accepted = Boolean(commandResult?.ok && result?.ok !== false);
  const status = accepted
    ? String(result.status || 'queued')
    : String(result.status || commandResult?.status || 'pending');
  const commandId = result.command_id || commandResult?.command_id || payload.record_id;
  const taskId = result.task_id || commandResult?.task_id || '';
  const taskStatus = result.task_status || commandResult?.task_status || status;
  const item = {
    id: `${payload.record_id}_${Date.now()}`,
    recordId: payload.record_id,
    commandId,
    taskId,
    taskStatus,
    moduleId: 'matching',
    column: payload.column,
    entityType: payload.entity_type,
    sourceType: payload.source_type,
    sourceLabel: describeImportSource(payload),
    status: taskStatus,
    state: accepted ? normalizeTaskState(taskStatus) : status === 'failed' ? 'failed' : 'pending',
    createdAt: new Date().toISOString(),
    error: commandResult?.error || ''
  };
  const next = [item, ...readImportStatuses().filter((existing) => existing.recordId !== payload.record_id)];
  writeImportStatuses(next);
}

function normalizeTaskState(status) {
  const value = String(status || '').toLowerCase();
  if (['running', 'leased', 'working'].includes(value)) return 'running';
  if (['completed', 'done', 'handled'].includes(value)) return 'completed';
  if (['failed', 'blocked', 'cancelled'].includes(value)) return 'failed';
  return 'queued';
}

function describeImportSource(payload) {
  const source = payload?.source || {};
  if (payload?.source_type === 'url') return source.url || 'URL ohne Adresse';
  if (payload?.source_type === 'document' || payload?.source_type === 'excel') {
    const files = Array.isArray(source.files) ? source.files : [];
    if (files.length === 1) return files[0].name || '1 Datei';
    if (files.length > 1) return `${files.length} Dateien`;
    return payload.source_type === 'excel' ? 'Excel/CSV Quelle' : 'Dokumentquelle';
  }
  if (payload?.source_type === 'text') return source.title || source.text?.slice(0, 80) || 'Freitext';
  return 'Quelle';
}

function renderImportStatuses() {
  const columns = ['requirements', 'matches', 'objects'];
  const items = readImportStatuses();
  for (const column of columns) {
    const host = ensureImportStatusHost(column);
    if (!host) continue;
    const columnItems = items.filter((item) => item.column === column).slice(0, 3);
    host.innerHTML = columnItems.map(renderImportStatusCard).join('');
    host.hidden = !columnItems.length;
    bindImportStatusCards(host);
  }
}

function ensureImportStatusHost(column) {
  const root = getMatchingRoot();
  const selectors = {
    requirements: '#left .sources',
    matches: '#center #requirementList',
    objects: '#right #objectList'
  };
  const anchor = root.querySelector(selectors[column]);
  if (!anchor?.parentElement) return null;
  let host = root.querySelector(`[data-import-status-host="${column}"]`);
  if (!host) {
    host = document.createElement('div');
    host.className = 'import-status-host';
    host.dataset.importStatusHost = column;
    anchor.parentElement.insertBefore(host, anchor);
  }
  return host;
}

function renderImportStatusCard(item) {
  const meta = getColumnMeta(item.column);
  const stateLabel = item.state === 'failed'
    ? 'Fehler'
    : item.state === 'pending'
      ? 'Ausstehend'
      : item.state === 'running'
        ? 'Running'
        : item.state === 'completed'
          ? 'Done'
          : 'Queued';
  const summary = item.state === 'failed'
    ? 'CTOX konnte den Import-Command nicht annehmen.'
    : item.state === 'pending'
      ? 'Lokal vorgemerkt. Wartet auf CTOX Verbindung oder Executor.'
      : item.state === 'running'
        ? 'CTOX verarbeitet diesen Import gerade.'
        : item.state === 'completed'
          ? 'CTOX hat den Task abgeschlossen; Ergebnis wird synchronisiert.'
          : 'Wartet auf CTOX Parser und schreibt danach JSON in RxDB.';
  const created = item.createdAt ? new Date(item.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : '';
  return `
    <article class="import-status-card" data-import-status="${escapeDrawerHtml(item.id)}" data-ctox-task-id="${escapeDrawerHtml(item.taskId || '')}" data-ctox-command-id="${escapeDrawerHtml(item.commandId || '')}" role="button" tabindex="0" title="Im CTOX Harness öffnen">
      <div class="import-status-topline">
        <strong>${escapeDrawerHtml(meta.singular || meta.label || 'Import')} Import</strong>
        <span class="import-status-pill import-status-${escapeDrawerHtml(item.state || 'queued')}">${escapeDrawerHtml(stateLabel)}</span>
      </div>
      <div class="import-status-source">${escapeDrawerHtml(item.sourceLabel || 'Quelle')}</div>
      <div class="import-status-summary">${escapeDrawerHtml(summary)}</div>
      <div class="import-status-meta">
        <span>${escapeDrawerHtml(item.sourceType || 'source')}</span>
        <span>${escapeDrawerHtml(item.taskId || item.commandId || item.recordId || 'command')}</span>
        <span>${escapeDrawerHtml(created)}</span>
      </div>
      ${item.error ? `<div class="import-status-error">${escapeDrawerHtml(item.error)}</div>` : ''}
    </article>
  `;
}

function bindImportStatusCards(host) {
  host.querySelectorAll('[data-import-status]').forEach((card) => {
    const open = () => openCtoxHarnessForTask(card.dataset.ctoxTaskId, card.dataset.ctoxCommandId);
    card.addEventListener('click', open);
    card.addEventListener('keydown', (event) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        open();
      }
    });
  });
}

function openCtoxHarnessForTask(taskId, commandId) {
  const focus = {
    taskId: taskId || '',
    commandId: commandId || '',
    sourceModule: 'matching',
    openedAt: Date.now()
  };
  const params = new URLSearchParams();
  if (focus.taskId) params.set('task_id', focus.taskId);
  if (focus.commandId) params.set('command_id', focus.commandId);
  const hash = params.toString() ? `ctox?${params.toString()}` : 'ctox';
  try {
    parent.sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify(focus));
    parent.location.hash = hash;
  } catch {
    try {
      sessionStorage.setItem('ctox.businessOs.focusTask', JSON.stringify(focus));
    } catch {}
    location.hash = hash;
  }
}

function initContextMenu() {
  const matchingRoot = getMatchingRoot();
  const root = matchingRoot.querySelector('.app') || matchingRoot;
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu';
  menu.hidden = true;
  root.append(menu);

  root.addEventListener('contextmenu', (event) => {
    const target = event.target;
    if (!target || target.closest?.('.ctox-context-menu')) return;
    event.preventDefault();
    const context = commandContextFromElement(target);
    renderContextMenu(menu, context, event.clientX, event.clientY);
  });

  document.addEventListener('click', () => {
    menu.hidden = true;
  });
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') menu.hidden = true;
  });
}

function renderContextMenu(menu, context, x, y) {
  const label = context.column ? getColumnMeta(context.column).label : 'Matching';
  menu.innerHTML = `
    <form class="matching-context-chat" data-matching-context-form>
      <header>
        <div>
          <strong>Chat to CTOX</strong>
          <span>${escapeDrawerHtml(label)} · ${escapeDrawerHtml(context.entityType || 'workspace')}</span>
        </div>
        <button type="button" data-context-close aria-label="Schließen">×</button>
      </header>
      <div class="matching-context-mode" role="radiogroup" aria-label="CTOX Aufgabe">
        <label><input type="radio" name="contextMode" value="data" checked /> Mit Daten arbeiten</label>
        <label><input type="radio" name="contextMode" value="app" /> App modifizieren</label>
      </div>
      <textarea data-context-message placeholder="Was soll CTOX hier tun oder prüfen?"></textarea>
      <footer>
        <span data-context-status></span>
        <button type="submit">Senden</button>
      </footer>
    </form>
  `;

  menu.hidden = false;
  const rect = menu.getBoundingClientRect();
  const left = Math.min(x, window.innerWidth - rect.width - 8);
  const top = Math.min(y, window.innerHeight - rect.height - 8);
  menu.style.left = `${Math.max(8, left)}px`;
  menu.style.top = `${Math.max(8, top)}px`;
  const form = menu.querySelector('[data-matching-context-form]');
  const textarea = menu.querySelector('[data-context-message]');
  const status = menu.querySelector('[data-context-status]');
  menu.querySelector('[data-context-close]')?.addEventListener('click', () => {
    menu.hidden = true;
  });
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const instruction = String(textarea?.value || '').trim();
    if (!instruction) {
      textarea?.focus();
      return;
    }
    const mode = new FormData(form).get('contextMode') || 'data';
    status.textContent = 'Sende...';
    const result = await dispatchCtoxCommand({
      module: 'matching',
      type: mode === 'app' ? 'business_os.app.modify' : 'matching.ctox.chat',
      record_id: context.fieldId || context.column || 'matching',
      payload: {
        instruction,
        mode,
        context
      },
      client_context: {
        action: 'context-chat',
        column: context.column,
        entity_type: context.entityType
      }
    });
    status.textContent = result?.ok === false ? 'Nicht angenommen.' : 'Gesendet.';
    if (result?.ok !== false) {
      setTimeout(() => { menu.hidden = true; }, 650);
    }
  });
  requestAnimationFrame(() => textarea?.focus());
}

function buildContextActions(context) {
  const label = context.column ? getColumnMeta(context.column).label : 'App';
  const actions = [
    {
      label: 'App modifizieren',
      type: 'business_os.app.modify',
      action: 'modify_app',
      requested_change: 'Modify this Business OS module based on the selected UI context.'
    }
  ];

  if (context.column) {
    actions.push(
      {
        label: `${label} konfigurieren`,
        type: 'business_os.column.configure',
        action: 'configure_column'
      },
      {
        label: `Parser und Datenstruktur für ${label} anpassen`,
        type: 'business_os.definition.modify',
        action: 'modify_parser_schema'
      },
      {
        label: `Suche, Filter und Sortierung für ${label} anpassen`,
        type: 'business_os.search_sort.modify',
        action: 'modify_search_sort'
      }
    );
  }

  if (context.importSource) {
    actions.push({
      label: `Importtyp ${context.importSource} anpassen`,
      type: 'business_os.import.modify',
      action: 'modify_import_source'
    });
  }

  if (context.fieldTag === 'input' || context.fieldTag === 'textarea' || context.fieldTag === 'select') {
    actions.push({
      label: 'Dieses Feld modifizieren',
      type: 'business_os.field.modify',
      action: 'modify_field'
    });
  }

  if (context.column === 'matches' || context.role === 'match-item') {
    actions.push({
      label: 'Scoring-Regeln anpassen',
      type: 'business_os.scoring.modify',
      action: 'modify_scoring'
    });
  }

  return actions;
}

function renderColumnDrawer(button) {
  const column = button.dataset.column || 'matches';
  const action = button.dataset.columnAction || 'configure';
  const meta = getColumnMeta(column);
  const actions = {
    configure: 'Konfiguration',
    'search-sort': 'Suche und Sortierung',
    import: 'Import',
    export: 'Export'
  };
  const title = `${meta.label || meta.plural || 'Spalte'} · ${actions[action] || 'Aktion'}`;
  const body = action === 'configure'
    ? renderConfigureDrawer(column)
    : action === 'search-sort'
      ? renderSearchSortDrawer(column)
    : action === 'import'
      ? renderImportDrawer(column)
      : renderExportDrawer(column);

  return `
    <div class="column-drawer-header">
      <strong data-column-drawer-title>${escapeDrawerHtml(title)}</strong>
      <button class="column-icon" type="button" data-column-drawer-close aria-label="Schließen" title="Schließen">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M18.3 5.7 12 12l6.3 6.3-1.4 1.4-6.3-6.3-6.3 6.3-1.4-1.4L9.2 12 2.9 5.7l1.4-1.4 6.3 6.3 6.3-6.3 1.4 1.4Z"/></svg>
      </button>
    </div>
    <div class="column-drawer-body">${body}</div>
  `;
}

function renderConfigureDrawer(column) {
  const config = COLUMN_PROMPTS[column] || COLUMN_PROMPTS.matches;
  const meta = getColumnMeta(column);
  const storageConfig = {
    ...(config.storage || {}),
    entityType: meta.entityType,
    labels: {
      label: meta.label,
      singular: meta.singular,
      plural: meta.plural
    }
  };
  return `
    <div class="drawer-grid">
      <label class="drawer-field">
        <span>Spaltenname</span>
        <input type="text" value="${escapeDrawerHtml(meta.label)}" data-column-config-field="label" />
      </label>
      <label class="drawer-field">
        <span>Entity Type</span>
        <input type="text" value="${escapeDrawerHtml(meta.entityType)}" data-column-config-field="entityType" />
      </label>
      <label class="drawer-field">
        <span>Typ Singular</span>
        <input type="text" value="${escapeDrawerHtml(meta.singular)}" data-column-config-field="singular" />
      </label>
      <label class="drawer-field">
        <span>Typ Plural</span>
        <input type="text" value="${escapeDrawerHtml(meta.plural)}" data-column-config-field="plural" />
      </label>
    </div>
    <label class="drawer-field">
      <span>Parser</span>
      <select>
        <option>${escapeDrawerHtml(config.parser)}</option>
      </select>
    </label>
    <label class="drawer-field">
      <span>Prompt</span>
      <textarea class="drawer-code" rows="12" spellcheck="false">${escapeDrawerHtml(config.prompt)}</textarea>
    </label>
    <label class="drawer-field">
      <span>Datenstruktur JSON</span>
      <textarea class="drawer-code" rows="14" spellcheck="false">${escapeDrawerHtml(JSON.stringify(config.schema, null, 2))}</textarea>
    </label>
    <label class="drawer-field">
      <span>RxDB Stammdaten JSON</span>
      <textarea class="drawer-code" rows="12" spellcheck="false">${escapeDrawerHtml(JSON.stringify(storageConfig, null, 2))}</textarea>
    </label>
    <label class="drawer-field">
      <span>Anzeige DSL</span>
      <textarea class="drawer-code" rows="14" spellcheck="false">${escapeDrawerHtml(JSON.stringify(config.display, null, 2))}</textarea>
    </label>
    <label class="drawer-field">
      <span>Schema Quelle</span>
      <select>
        <option>${escapeDrawerHtml(config.structure)}</option>
      </select>
    </label>
  `;
}

function renderImportDrawer(column) {
  const recordLabel = column === 'requirements'
    ? 'Anforderungen'
    : column === 'objects'
      ? 'Objekte'
      : 'Matches';
  const defaultSource = column === 'matches' ? 'excel' : 'document';
  return renderUniversalImportDrawerMarkup({ recordLabel, defaultSource });
}

function buildDefaultSearchSortConfig(column) {
  const config = COLUMN_PROMPTS[column] || COLUMN_PROMPTS.matches;
  const meta = getColumnMeta(column);
  const display = config.display || {};
  const filterDefaults = column === 'requirements'
    ? [
        { id: 'source', label: 'Quellen', type: 'facet', field: 'source.name' },
        { id: 'location', label: 'Standort', type: 'facet', field: 'requirement.location' },
        { id: 'work_model', label: 'Arbeitsmodell', type: 'facet', field: 'requirement.workModel' },
        { id: 'urgency', label: 'Dringlichkeit', type: 'range', field: 'requirementSource.parsed.urgencyValue', min: 0, max: 3 }
      ]
    : column === 'objects'
      ? [
          { id: 'skills', label: 'Skills', type: 'token', field: 'object.skills' },
          { id: 'location', label: 'Ort / Region', type: 'facet', field: 'object.region' },
          { id: 'status', label: 'Status', type: 'facet', field: 'object.objectStatus' },
          { id: 'availability', label: 'Verfügbarkeit', type: 'date', field: 'object.availabilityFrom' }
        ]
      : [
          { id: 'score', label: 'Score', type: 'range', field: 'match.score', min: 0, max: 100 },
          { id: 'priority', label: 'Kriteriengruppe', type: 'facet', field: 'match.items.priority' },
          { id: 'conflict', label: 'Konflikte', type: 'boolean', expression: 'hasConflict(match.items)' },
          { id: 'status', label: 'Status', type: 'facet', field: 'match.status' }
        ];

  return {
    instructions: `CTOX soll für die Spalte "${meta.label}" passende Suche, Filter und Sortierungen aus der JSON-Struktur ableiten. Die Konfiguration soll auf business_records.data arbeiten, robuste Feldpfade verwenden und nur abgeleitete Indexfelder nutzen, wenn sie aus data reproduzierbar sind.`,
    search: {
      placeholder: display.search?.placeholder || `${meta.singular || meta.label} suchen...`,
      mode: 'fuzzy + exact phrase',
      fields: display.search?.fields || [],
      tokenizer: 'language-aware',
      emptyState: `Keine ${meta.plural || meta.label} im aktuellen Filter gefunden.`
    },
    filters: filterDefaults,
    sort: display.sort || []
  };
}

function getSearchSortConfig(column) {
  const defaults = buildDefaultSearchSortConfig(column);
  const stored = readSearchSortSettings()[column] || {};
  return {
    instructions: stored.instructions || defaults.instructions,
    search: stored.searchJson || JSON.stringify(defaults.search, null, 2),
    filters: stored.filtersJson || JSON.stringify(defaults.filters, null, 2),
    sort: stored.sortJson || JSON.stringify(defaults.sort, null, 2)
  };
}

function renderSearchSortDrawer(column) {
  const meta = getColumnMeta(column);
  const config = getSearchSortConfig(column);
  return `
    <label class="drawer-field">
      <span>CTOX Auftrag</span>
      <textarea rows="7" data-search-sort-field="instructions">${escapeDrawerHtml(config.instructions)}</textarea>
    </label>
    <label class="drawer-field">
      <span>Suchdefinition JSON</span>
      <textarea class="drawer-code" rows="9" spellcheck="false" data-search-sort-field="searchJson">${escapeDrawerHtml(config.search)}</textarea>
    </label>
    <label class="drawer-field">
      <span>Filterdefinition JSON</span>
      <textarea class="drawer-code" rows="12" spellcheck="false" data-search-sort-field="filtersJson">${escapeDrawerHtml(config.filters)}</textarea>
    </label>
    <label class="drawer-field">
      <span>Sortierungen JSON</span>
      <textarea class="drawer-code" rows="10" spellcheck="false" data-search-sort-field="sortJson">${escapeDrawerHtml(config.sort)}</textarea>
    </label>
    <button class="drawer-primary" type="button" data-search-sort-save>Konfiguration für ${escapeDrawerHtml(meta.label)} speichern</button>
  `;
}

function renderExportDrawer(column) {
  const scope = column === 'requirements' ? 'Anforderungen' : column === 'objects' ? 'Objekte' : 'Matches';
  return `
    <label class="drawer-field">
      <span>Umfang</span>
      <select>
        <option>${escapeDrawerHtml(scope)} im aktuellen Filter</option>
        <option>Alle ${escapeDrawerHtml(scope)}</option>
      </select>
    </label>
    <label class="drawer-field">
      <span>Format</span>
      <select>
        <option>JSON</option>
        <option>CSV</option>
        <option>XLSX</option>
      </select>
    </label>
    <button class="drawer-primary" type="button">Export vorbereiten</button>
  `;
}

function escapeDrawerHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
