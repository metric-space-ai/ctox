// core/onboarding.js — pure per-record checklist with completion gating.
// No DOM, no RxDB.
//
// Baukasten note: a generic checklist engine. Recruiting maps it to the
// pre-start/first-day onboarding (documents complete, Sicherheitsunterweisung,
// PSA issued, access granted); another vertical supplies its own item set.

/** Default recruiting onboarding items (config). */
export const ONBOARDING_ITEMS = [
  { key: 'documents_complete', label: 'Unterlagen vollständig', required: true },
  { key: 'sicherheitsunterweisung', label: 'Sicherheitsunterweisung', required: true },
  { key: 'psa_issued', label: 'PSA/Arbeitsmittel ausgegeben', required: false },
  { key: 'system_access', label: 'Systemzugang eingerichtet', required: false },
];

/** Normalize a checklist state into {key: boolean}. */
export function normalizeChecklist(state) {
  const source = state && typeof state === 'object' ? state : {};
  const out = {};
  for (const item of ONBOARDING_ITEMS) out[item.key] = Boolean(source[item.key]);
  return out;
}

/** Progress over the item set. */
export function checklistProgress(state, items = ONBOARDING_ITEMS) {
  const normalized = state && typeof state === 'object' ? state : {};
  const total = items.length;
  const done = items.filter((item) => Boolean(normalized[item.key])).length;
  return { done, total, complete: done === total };
}

/** Handoff gate: every REQUIRED item must be checked. */
export function evaluateOnboardingGate(state, items = ONBOARDING_ITEMS) {
  const normalized = state && typeof state === 'object' ? state : {};
  const blockers = items
    .filter((item) => item.required && !normalized[item.key])
    .map((item) => ({ key: item.key, reason: 'required_item_open' }));
  return { allowed: blockers.length === 0, blockers };
}
