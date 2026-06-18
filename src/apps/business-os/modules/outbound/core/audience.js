// core/audience.js — pure audience-targeting model that lets the outbound
// sequenced-outreach engine address candidates as well as companies. No DOM,
// no RxDB.
//
// Baukasten note: outbound is a generic multi-step sequence engine. This model
// generalizes its audience from companies to any entity (here: the candidate
// pool) via a saved-search predicate + tags, and validates the send channel.
// The engine, sequences, approvals and suppression are reused unchanged.

export const AUDIENCE_ENTITY_TYPES = ['company', 'candidate'];

/** Channels the candidate audience can be reached on. */
export const CANDIDATE_CHANNELS = ['email', 'whatsapp', 'sms', 'inmail'];

export function isCandidateChannel(channel) {
  return CANDIDATE_CHANNELS.includes(String(channel));
}

/** Talent-pool reactivation tags (config). */
export const TALENT_POOL_TAGS = ['silver_medalist', 'ex_zeitarbeitnehmer', 'aktiv_suchend', 'passiv'];

/**
 * Evaluate one saved-search criterion against a candidate record.
 * criterion: { field, op: 'includes'|'equals'|'has_tag'|'gte'|'lte', value }
 */
function criterionMatches(candidate, criterion) {
  const source = candidate && typeof candidate === 'object' ? candidate : {};
  if (criterion?.op === 'has_tag') {
    const tags = Array.isArray(source.tags) ? source.tags.map((t) => String(t)) : [];
    return tags.includes(String(criterion.value));
  }
  const value = source[criterion?.field];
  switch (criterion?.op) {
    case 'equals':
      return String(value).toLowerCase() === String(criterion.value).toLowerCase();
    case 'gte':
      return Number(value) >= Number(criterion.value);
    case 'lte':
      return Number(value) <= Number(criterion.value);
    case 'includes': {
      const needle = String(criterion.value).toLowerCase();
      if (Array.isArray(value)) return value.some((v) => String(v).toLowerCase().includes(needle));
      return String(value || '').toLowerCase().includes(needle);
    }
    default:
      return false;
  }
}

/** A saved search matches when every criterion matches (AND). */
export function matchesSavedSearch(candidate, savedSearch) {
  const criteria = Array.isArray(savedSearch?.criteria) ? savedSearch.criteria : [];
  if (!criteria.length) return false;
  return criteria.every((criterion) => criterionMatches(candidate, criterion));
}

/**
 * Build an outreach audience from the candidate pool. Honors a saved search and
 * a suppression set, and only includes candidates reachable on the channel.
 * @param {Array<object>} candidates
 * @param {{ savedSearch?: object, channel?: string, suppressedIds?: Set<string>|string[] }} opts
 */
export function buildCandidateAudience(candidates, { savedSearch, channel = 'email', suppressedIds } = {}) {
  const suppressed = suppressedIds instanceof Set ? suppressedIds : new Set(suppressedIds || []);
  const channelOk = isCandidateChannel(channel);
  const reachField = { email: 'email', whatsapp: 'phone', sms: 'phone', inmail: 'linkedin' }[channel];
  return (Array.isArray(candidates) ? candidates : []).filter((candidate) => {
    if (!candidate || suppressed.has(candidate.id)) return false;
    if (!channelOk) return false;
    if (reachField && !candidate[reachField]) return false;
    if (savedSearch && !matchesSavedSearch(candidate, savedSearch)) return false;
    return true;
  });
}
