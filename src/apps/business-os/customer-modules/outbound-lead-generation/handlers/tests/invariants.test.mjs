// Geschäfts-Invarianten 5–6 der App outbound-lead-generation (Strang INV, Shell v4).
// Der Handler outbound.lead.research_writeback ist noch legacy (Port läuft in Strang OLG3):
// beide Tests prüfen die bereits portierten _lib-Funktionen, über die der Handler die
// Invariante halten wird. WARTET AUF OLG3: sobald der Handler portiert ist, denselben Fall
// über handle(command, ctx) gegen tests/_ctx.mjs fahren (Lead-Datensatz statt Patch-Objekt).
// Je Invariante genau EIN Test; Mutationsbeleg: tests/mutation-proof.mjs, Bericht: INVARIANTS.md.
import test from 'node:test';
import assert from 'node:assert/strict';
import { researchOutcomePatch } from '../_lib/research-outcome-patch.js';
import { matchingResearchEvidenceCount } from '../_lib/research-evidence.js';
import { parseResearchWritebackRequest } from '../_lib/research-writeback-payload.js';
import { rejectEvidenceFreeWriteback } from '../_lib/research-writeback-checks.js';
import { sanitizeResearchWriteback } from '../_lib/research-writeback-sanitize.js';
import { CrmBaseline } from '../_lib/research-validation.js';

const LEAD = {
  id: 'lead-evidence-binding',
  contacts: [
    { id: 'contact-a', person_key: 'person-a', name: 'Ada Lovelace' },
    { id: 'contact-b', person_key: 'person-b', name: 'Grace Hopper' },
  ],
  data: { firma_ort: 'Berlin' },
  evidence: [],
};

function patchWith(fieldKey, current, evidenceEntry) {
  const existing = structuredClone(LEAD);
  existing.evidence = [evidenceEntry];
  const field = { value: current };
  if (fieldKey.startsWith('person_')) field.person_key = 'person-b';
  return researchOutcomePatch(existing, { fields: { [fieldKey]: field } }, 2_000);
}

// ---------------------------------------------------------------------------
// INV5 – Ein Beleg bestätigt nie eine andere Person.
// Rust: historical_evidence_never_certifies_another_person_or_current_value
//       (person_research_command.rs:3445).
test('INV5 evidence never certifies another person', () => {
  const quote = 'Contact: team(at)example.test';
  const evidence = (personKey) => ({
    field_key: 'person_email', value: 'team@example.test', person_key: personKey, quote,
    source_id: 'company-website', source_url: 'https://example.test/contact',
  });

  // (a) identical value + quote, but the evidence belongs to person A, the field to person B
  const foreign = patchWith('person_email', 'team@example.test', evidence('person-a'));
  assert.deepEqual(foreign.payload.verified_field_keys, [], 'evidence of person A must not verify person B');
  assert.equal(foreign.research_status, 'needs_review');
  assert.equal(foreign.contacts[0].name, 'Ada Lovelace');
  assert.equal(foreign.contacts[1].name, 'Grace Hopper');

  // (b) evidence without any person binding does not verify a person-bound field either
  assert.equal(matchingResearchEvidenceCount([evidence(undefined)], 'person_email',
    { value: 'team@example.test', person_key: 'person-b' }, false), 0);

  // control: the same evidence bound to person B verifies person B
  const own = patchWith('person_email', 'team@example.test', evidence('person-b'));
  assert.deepEqual(own.payload.verified_field_keys, ['person_email']);
  assert.equal(own.research_status, 'completed');
});

// ---------------------------------------------------------------------------
// INV6 – Ein beleglos zurückgeschriebenes Ergebnis schließt keinen Lead ab.
// Rust: a_writeback_without_any_evidence_is_rejected (person_research_gap_closure.rs:5950,
//       Gate :1707), completed_research_requires_durable_lead_writeback (person_research_command.rs:2636,
//       Statusregel im Ergebnispatch :555 ff.).
test('INV6 an evidence-free writeback never completes a lead', () => {
  // (a) writeback gate: only no_match without sources/attempts -> rejected before anything is written
  const evidenceFree = parseResearchWritebackRequest({
    record_id: 'lead-1', module: 'outbound-lead-generation', research_command_id: 'research-1',
    field_status: {
      firma_name: { status: 'no_match', value: null, sources: [], attempts: [], reason: 'Keine unabhängige Quelle gefunden.' },
      firma_domain: { status: 'no_match', value: null, sources: [], attempts: [] },
    },
  });
  assert.throws(() => rejectEvidenceFreeWriteback(evidenceFree), /evidence-free research writeback rejected/);

  // (b) result patch: researched values without a matching source never set research_status=completed
  const unbacked = researchOutcomePatch(structuredClone(LEAD), {
    fields: {
      firma_name: { value: 'Beispiel GmbH' },
      firma_ort: { value: 'Bielefeld' },
    },
  }, 2_000);
  assert.equal(unbacked.research_status, 'needs_review');
  assert.deepEqual(unbacked.payload.verified_field_keys, []);

  // (c) a stale source for the OLD value does not back the new value
  const stale = patchWith('firma_ort', 'Bielefeld', {
    field_key: 'firma_ort', value: 'Berlin', quote: 'Berlin', source_id: 'company-website', source_url: 'https://example.test/contact',
  });
  assert.equal(stale.research_status, 'needs_review');

  // (d) "verified" without any source passes the coarse gate (any field verified), but the
  //     writeback sanitizer demotes it and records a rejection; the handler's terminal rule
  //     (Rust person_research_gap_closure.rs:2032 `!rejections.is_empty()` -> needs_review)
  //     therefore keeps the lead open. WARTET AUF OLG3: Terminalregel im portierten Handler.
  const claimedOnly = parseResearchWritebackRequest({
    record_id: 'lead-1', module: 'outbound-lead-generation', research_command_id: 'research-1',
    field_status: { firma_name: { status: 'verified', value: 'Beispiel GmbH' } },
  });
  assert.doesNotThrow(() => rejectEvidenceFreeWriteback(claimedOnly));
  const rejections = sanitizeResearchWriteback(claimedOnly, ['firma_name'], CrmBaseline.fromResearchPayload({}));
  assert.notEqual(claimedOnly.field_status.firma_name.status, 'verified', 'a sourceless verified claim is demoted');
  assert.ok(rejections.length > 0, 'the demotion is recorded as a rejection (-> needs_review)');

  // control: one matching, cited source completes the lead (owner rule: one source suffices)
  const backed = patchWith('firma_ort', 'Bielefeld', {
    field_key: 'firma_ort', value: 'Bielefeld', quote: 'Sitz: Bielefeld', source_id: 'company-website', source_url: 'https://example.test/imprint',
  });
  assert.equal(backed.research_status, 'completed');
});
