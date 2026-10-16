import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import * as lib from '../_lib.js';

const RUST_TYPES = [
  'customers.account.create', 'customers.account.update', 'customers.account.archive',
  'customers.contact.create', 'customers.contact.update', 'customers.contact.archive',
  'customers.opportunity.create', 'customers.opportunity.update', 'customers.opportunity.move_stage',
  'customers.opportunity.close_won', 'customers.opportunity.close_lost',
  'customers.task.create', 'customers.task.update', 'customers.task.complete',
  'customers.note.create', 'customers.note.update', 'customers.activity.record', 'customers.view.save',
  'customers.import.from_outbound', 'customers.dedupe.resolve',
];

test('jeder Rust-Befehlstyp (store.rs:21445) hat genau eine Handler-Datei', () => {
  const dir = fileURLToPath(new URL('..', import.meta.url));
  const files = readdirSync(dir).filter((f) => f.endsWith('.js') && !f.startsWith('_')).map((f) => f.slice(0, -3)).sort();
  assert.deepEqual(files, [...RUST_TYPES].sort());
});

test('jede Handler-Datei exportiert default async handle(command, ctx)', async () => {
  for (const type of RUST_TYPES) {
    const mod = await import(`../${type}.js`);
    assert.equal(typeof mod.default, 'function', type);
    assert.equal(mod.default.length, 2, type);
  }
});

test('stableDigest = sha256 hex[..24] wie channels::stable_digest', () => {
  for (const s of ['', 'cmd_customer_account_create', 'Grüße 😀', 'x'.repeat(200)]) {
    assert.equal(lib.stableDigest(s), createHash('sha256').update(s).digest('hex').slice(0, 24));
  }
});

test('str trimmt und verwirft Leeres und Nicht-Strings (outbound_string)', () => {
  assert.equal(lib.str({ a: '  x ' }, ['a']), 'x');
  assert.equal(lib.str({ a: '   ' }, ['a']), undefined);
  assert.equal(lib.str({ a: 5 }, ['a']), undefined);
  assert.equal(lib.str({ a: { b: 'y' } }, ['a', 'b']), 'y');
  assert.equal(lib.str(null, ['a']), undefined);
});

test('idFromCommand: Payload vor record_id vor prefix_digest(id)', () => {
  assert.equal(lib.idFromCommand({ id: 'c', payload: { account_id: ' a1 ' }, record_id: 'r' }, ['account_id'], 'acct'), 'a1');
  assert.equal(lib.idFromCommand({ id: 'c', payload: {}, record_id: ' r ' }, ['account_id'], 'acct'), 'r');
  assert.equal(lib.idFromCommand({ id: 'c', payload: {} }, ['account_id'], 'acct'), `acct_${lib.stableDigest('c')}`);
  assert.throws(() => lib.idFromCommand({ payload: {} }, ['x'], 'p'), { code: 'invalid', message: 'record id is required' });
});

test('Default-Setzer folgen serde-Semantik', () => {
  const r = { s: ' ', i: 1.5, b: 'true', o: [] };
  lib.putDefaultString(r, 's', 'd');
  lib.putDefaultI64(r, 'i', 7);
  lib.putDefaultBool(r, 'b', false);
  lib.putDefaultObject(r, 'o');
  assert.deepEqual(r, { s: 'd', i: 7, b: false, o: {} });
});

test('mergeFields übernimmt nur gelistete Schlüssel, auch null', () => {
  const r = { a: 1, b: 2 };
  lib.mergeFields(r, { a: null, c: 3 }, ['a', 'c', 'd']);
  assert.deepEqual(r, { a: null, b: 2, c: 3 });
});

test('refreshSearchText senkt nur ASCII (to_ascii_lowercase)', () => {
  const r = { name: 'ÄBC GmbH', domain: ' Acme.DE ', industry: '' };
  lib.refreshSearchText(r, ['name', 'domain', 'industry']);
  assert.equal(r.search_text, 'Äbc gmbh acme.de');
});

test('domainFromWebsite wie Url::parse + trim_start_matches("www.")', () => {
  assert.equal(lib.domainFromWebsite('https://www.Acme.example/x'), 'acme.example');
  assert.equal(lib.domainFromWebsite('www.www.acme.example'), 'acme.example');
  assert.equal(lib.domainFromWebsite('acme.example:8080/path?q'), 'acme.example');
  assert.equal(lib.domainFromWebsite('   '), undefined);
  assert.equal(lib.domainFromWebsite('http://'), undefined);
});

test('splitName und contactDisplayName', () => {
  assert.deepEqual(lib.splitName('  Grace  Brewster Hopper '), ['Grace', 'Brewster Hopper']);
  assert.deepEqual(lib.splitName(''), ['', '']);
  assert.equal(lib.contactDisplayName({ first_name: 'Ada', last_name: '' }), 'Ada');
  assert.equal(lib.contactDisplayName({ email: 'a@b.c' }), 'a@b.c');
  assert.equal(lib.contactDisplayName({}), 'Kontakt');
});

test('Fehler sind {code, message}', () => {
  assert.throws(() => lib.validateAllowed('task status', 'x', ['open']), {
    code: 'invalid', message: 'task status `x` is not supported',
  });
  assert.throws(() => lib.validateStageTransition('closed_won', 'proposal'), { code: 'conflict' });
  assert.doesNotThrow(() => lib.validateStageTransition('closed_won', 'closed_won'));
});
