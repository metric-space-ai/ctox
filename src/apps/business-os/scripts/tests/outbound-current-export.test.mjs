import assert from 'node:assert/strict';
import { test } from 'node:test';
import { webcrypto } from 'node:crypto';
import { captureResearchExport, openResearchSnapshot } from '../../customer-modules/outbound-lead-generation/current-state-export.mjs';

test('click snapshot freezes revisions, evidence, values and judgments without modifying inputs', () => {
  const live = [{ id: 'a', _rev: '1-a', data: { firma_name: 'Firma' }, evidence: [{ quote: 'Firma' }],
    contacts: [{ person_key: 'person-a', person_vorname: 'A', person_nachname: 'B', person_email: 'a@example.test' }] }];
  const judgment = { status: 'free', reason: 'saved' };
  const snapshot = captureResearchExport(live, () => judgment, 123);
  live[0].data.firma_name = 'new'; live[0].evidence[0].quote = 'new'; judgment.status = 'blocked';
  const lead = snapshot.leads[0];
  assert.equal(lead.data.firma_name, 'Firma'); assert.equal(lead.evidence[0].quote, 'Firma');
  assert.equal(snapshot.recipientStatus(lead, lead.contacts[0]).status, 'free');
  assert.deepEqual(snapshot.sourceRecordIds, ['a']); assert.equal(snapshot.capturedAt, 123);
  assert.equal('capturedRecipients' in live[0], false);
  const returned = snapshot.recipientStatus(lead, lead.contacts[0]); returned.status = 'new';
  assert.equal(snapshot.recipientStatus(lead, lead.contacts[0]).status, 'free');
});

test('contact judgments are lead scoped, alias aware and never matched by name alone', () => {
  const leads = ['a', 'b'].map(id => ({ id, contacts: [{ id: 'same', person_key: 'key', sellify_person_id: 42,
    person_vorname: 'A', person_nachname: 'B', person_email: 'a@example.test' }] }));
  const snapshot = captureResearchExport(leads, lead => ({ status: lead.id === 'a' ? 'free' : 'blocked' }));
  assert.equal(snapshot.recipientStatus(snapshot.leads[0], { sellify_person_id: 42 }).status, 'free');
  assert.equal(snapshot.recipientStatus(snapshot.leads[1], { id: 'same' }).status, 'blocked');
  assert.equal(snapshot.recipientStatus(snapshot.leads[0], { person_vorname: 'A', person_nachname: 'B' }), null);
  assert.equal(snapshot.recipientStatus(snapshot.leads[0], { person_vorname: 'A', person_nachname: 'B', email: 'a@example.test' }).status, 'free');
  const duplicate = captureResearchExport([{ id: 'x', contacts: [{ person_key: 'same' }, { person_key: 'same' }] }], () => ({ status: 'free' }));
  assert.equal(duplicate.recipientStatus(duplicate.leads[0], { person_key: 'same' }), null);
});

test('missing decisions remain untested; clone or resolver errors are not silently dropped', () => {
  const lead = { id: 'a', contacts: [{ id: 'c' }] };
  const snapshot = captureResearchExport([null, lead], () => null);
  assert.equal(snapshot.recipientStatus(snapshot.leads[0], { id: 'c' }), null);
  assert.throws(() => captureResearchExport([{ id: 'a', function: () => {} }], () => null));
  assert.throws(() => captureResearchExport([lead], () => { throw Error('failure'); }), /failure/);
});

test('Spreadsheet receives the exact downloaded bytes, click metadata and hash; no invented record URLs', async () => {
  const blob = new Blob(['current XLSX bytes'], { type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' });
  const snapshot = captureResearchExport([{ id: 'a', contacts: [] }, { id: 'a' }], () => null, 123);
  let received;
  await openResearchSnapshot({ openApp: (app, value) => { received = { app, value }; } }, blob, 'Recherche.xlsx', snapshot, webcrypto);
  assert.equal(received.app, 'spreadsheets');
  const file = received.value.openFile;
  assert.equal(file.file.name, 'Recherche.xlsx'); assert.equal(await file.file.text(), await blob.text());
  assert.equal(file.source_kind, 'research_generated'); assert.equal(file.open_purpose, 'snapshot_report');
  assert.equal(file.report_snapshot.captured_at_ms, 123); assert.deepEqual(file.report_snapshot.source_record_ids, ['a']);
  const digest = Buffer.from(await webcrypto.subtle.digest('SHA-256', await blob.arrayBuffer())).toString('hex');
  assert.equal(file.report_snapshot.file_sha256, digest);
  await assert.rejects(openResearchSnapshot({}, blob, 'a', snapshot, webcrypto), /nicht geöffnet/);
  await assert.rejects(openResearchSnapshot({ openApp() {} }, blob, 'a', { sourceRecordIds: [] }, webcrypto), /Lead-IDs/);
});
