// Generic interview-coordination collections: structured scorecards and
// multi-party meetings.

const interviewScorecardsSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    candidate_id: { type: 'string' },
    vacancy_id: { type: 'string' },
    role_template: { type: 'string' },
    criteria: { type: 'array', items: { type: 'object', additionalProperties: true } },
    ratings: { type: 'object', additionalProperties: true },
    overall: { type: 'number' },
    interviewer: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    _deleted: { type: 'boolean' },
  },
  required: ['id', 'updated_at_ms'],
  additionalProperties: true,
};

const interviewMeetingsSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    candidate_id: { type: 'string' },
    vacancy_id: { type: 'string' },
    parties: { type: 'array', items: { type: 'object', additionalProperties: true } },
    start: { type: 'number' },
    end: { type: 'number' },
    location_mode: { type: 'string' },
    video_link: { type: 'string' },
    state: { type: 'string' },
    transcript_id: { type: 'string' },
    attended: { type: 'boolean' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    _deleted: { type: 'boolean' },
  },
  required: ['id', 'updated_at_ms'],
  additionalProperties: true,
};

export const collections = {
  interview_scorecards: interviewScorecardsSchema,
  interview_meetings: interviewMeetingsSchema,
};

export const migrationStrategies = {};
