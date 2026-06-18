// core/application.js — pure multi-channel application intake normalization +
// dedupe. No DOM, no RxDB.
//
// Baukasten note: a generic "normalize an inbound record from many channels
// into one shape, detect duplicates" engine. Recruiting maps it to job
// applications feeding the candidate pool; another vertical reuses the funnel.

export const INTAKE_CHANNELS = ['career_site', 'job_board', 'easy_apply', 'email', 'qr', 'walk_in', 'referral'];

export function isIntakeChannel(channel) {
  return INTAKE_CHANNELS.includes(String(channel));
}

function cleanString(value) {
  return value === undefined || value === null ? '' : String(value).trim();
}

/**
 * Normalize a raw inbound application from any channel into one record.
 * @param {object} raw
 */
export function normalizeApplication(raw) {
  const source = raw && typeof raw === 'object' ? raw : {};
  const email = cleanString(source.email).toLowerCase();
  const name = cleanString(source.name || [source.firstName, source.lastName].filter(Boolean).join(' '));
  const documents = (Array.isArray(source.documents) ? source.documents : [])
    .map((doc) => ({ kind: cleanString(doc?.kind) || 'unknown', file_id: cleanString(doc?.file_id) }))
    .filter((doc) => doc.file_id);
  return {
    id: cleanString(source.id),
    channel: isIntakeChannel(source.channel) ? source.channel : 'email',
    vacancy_id: cleanString(source.vacancy_id),
    candidate: { name, email, phone: cleanString(source.phone) },
    documents,
    received_at_ms: Number(source.received_at_ms) || 0,
    status: 'new',
  };
}

/** Dedupe key for an application/candidate: email if present, else lowercased name. */
export function applicationDedupeKey(application) {
  const email = cleanString(application?.candidate?.email || application?.email).toLowerCase();
  if (email) return `email:${email}`;
  const name = cleanString(application?.candidate?.name || application?.name).toLowerCase();
  return name ? `name:${name}` : '';
}

/**
 * Find a likely duplicate of an application among existing records by dedupe
 * key. Returns the existing record or null.
 */
export function findDuplicateApplication(existing, application) {
  const key = applicationDedupeKey(application);
  if (!key) return null;
  return (Array.isArray(existing) ? existing : []).find((rec) => applicationDedupeKey(rec) === key) || null;
}
