import { recoveryJournalTestInternals } from '../src/recovery-journal.mjs';

const { countOutstandingWrites, masterAcknowledgesLocal } = recoveryJournalTestInternals;
const batches = [
  { documentIds: ['lead-a', 'lead-b'], ackedIds: ['lead-a'] },
  { documentIds: ['lead-a'], ackedIds: [] },
];
if (countOutstandingWrites(batches) !== 2) {
  throw new Error('pendingWrites must exclude acknowledged IDs but retain distinct historical versions');
}
if (countOutstandingWrites([{ documentIds: ['lead-a'], ackedIds: ['lead-a'] }]) !== 0) {
  throw new Error('a fully acknowledged batch has no outstanding writes');
}
const older = { id: 'lead-a', _meta: { ctoxHlc: '10:0:browser' } };
const newer = { id: 'lead-a', _meta: { ctoxHlc: '11:0:browser' } };
if (masterAcknowledgesLocal(newer, older, 'outbound_leads')) {
  throw new Error('a newer master version must not silently discard an older journaled edit');
}
if (!masterAcknowledgesLocal(newer, newer, 'outbound_leads')) {
  throw new Error('the exact acknowledged HLC must be eligible to drain');
}

console.log('ctox-rxdb recovery journal accounting smoke OK');
