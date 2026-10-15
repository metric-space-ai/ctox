// Google adapters (google-de, maps-google-com) through their real process
// boundary: `child_process` (ctox secret/web read) and `https` (Bright Data
// SERP API) are mocked. No provider request leaves the test process.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const SECRET = 'test-key-not-a-secret';

function serp(extra = {}) {
  return { general: { search_engine: 'google' }, organic: [], ...extra };
}

function fixture(file, input, { responses = [], ctox = () => ({ ok: false }), token = SECRET, targetDir } = {}) {
  const calls = [];
  const requests = [];
  const output = [];
  const queue = [...responses];
  const https = {
    request(options, onResponse) {
      const handlers = {};
      let body = '';
      const req = {
        on(event, fn) { handlers[event] = fn; return req; },
        write(chunk) { body += chunk; },
        destroy(error) { if (handlers.error) handlers.error(error); },
        end() {
          const parsed = JSON.parse(body);
          requests.push({ options, body: parsed });
          let next = queue.length > 0 ? queue.shift() : { status: 500, body: '' };
          if (typeof next === 'function') next = next(parsed);
          Promise.resolve().then(() => {
            if (next === 'timeout') { if (handlers.timeout) handlers.timeout(); return; }
            const resHandlers = {};
            const res = { statusCode: next.status, setEncoding() {}, on(event, fn) { resHandlers[event] = fn; return res; } };
            onResponse(res);
            const text = typeof next.body === 'string' ? next.body : JSON.stringify(next.body);
            if (text && resHandlers.data) resHandlers.data(text);
            if (resHandlers.end) resHandlers.end();
          });
        },
      };
      return req;
    },
  };
  const requireMock = (name) => {
    if (name === 'https') return https;
    if (name === 'path') return path;
    assert.equal(name, 'child_process');
    return { execFileSync: (bin, args, options) => {
      calls.push({ bin, args: Array.from(args), source: options && options.input });
      if (args[0] === 'secret' && args[1] === 'get') {
        if (!token) throw new Error('secret not found');
        return JSON.stringify({ value: token });
      }
      if (bin === 'sqlite3') throw new Error('no runtime store in tests');
      return JSON.stringify(ctox(Array.from(args)));
    } };
  };
  requireMock.main = {};
  const env = { CTOX_SCRAPE_INPUT_JSON: JSON.stringify(input) };
  if (targetDir) env.CTOX_SCRAPE_TARGET_DIR = targetDir;
  const context = {
    module: { exports: {} }, require: requireMock, URL, URLSearchParams, Buffer, setTimeout, clearTimeout,
    process: { env, stdout: { write: (value) => output.push(value) }, stderr: { write: () => {} } },
  };
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, file), 'utf8'), context);
  return {
    calls,
    requests,
    async run() { await context.main(); return JSON.parse(output.join('')); },
  };
}

const companyPage = (url, text) => (args) => (args[0] === 'web' && args[1] === 'read'
  ? { ok: true, url: args[args.indexOf('--url') + 1], title: 'Example', page_text_excerpt: text(args[args.indexOf('--url') + 1]) }
  : { ok: false });

// --- google.de: company mode -------------------------------------------------

test('google.de: company website from the Bright Data SERP, never a browser SERP', async () => {
  const f = fixture('google-de-authenticated.js', { company: 'Example Werke GmbH', country: 'AT' }, {
    responses: [{ status: 200, body: serp({ organic: [
      { link: 'https://www.northdata.de/Example', title: 'Example Werke GmbH - North Data', description: 'Example Werke GmbH' },
      { link: 'https://www.example-werke.at/', title: 'Example Werke GmbH', description: 'Example Werke GmbH, Linz' },
    ] }) }],
    ctox: companyPage('', (url) => (url.endsWith('/impressum')
      ? 'Impressum Example Werke GmbH office@example-werke.at Tel +43 732 123456'
      : 'Willkommen bei der Example Werke GmbH')),
  });
  const result = await f.run();
  assert.equal(result.failure_mode, undefined);
  const fields = Object.fromEntries(result.records.map((r) => [r.field, r.value]));
  assert.equal(fields.firma_name, 'Example Werke GmbH');
  assert.equal(fields.firma_domain, 'example-werke.at');
  assert.equal(fields.firma_email, 'office@example-werke.at');
  assert.equal(f.requests.length, 1);
  const request = f.requests[0];
  assert.equal(request.options.host, 'api.brightdata.com');
  assert.equal(request.options.path, '/request');
  assert.equal(request.options.headers.Authorization, `Bearer ${SECRET}`);
  assert.ok(!Object.keys(request.options.headers).some((h) => h.toLowerCase() === 'accept'));
  assert.equal(request.body.zone, 'serp_api1');
  assert.equal(request.body.format, 'raw');
  const serpUrl = new URL(request.body.url);
  assert.equal(serpUrl.searchParams.get('q'), 'Example Werke GmbH');
  assert.equal(serpUrl.searchParams.get('gl'), 'at');
  assert.equal(serpUrl.searchParams.get('brd_json'), '1');
  assert.ok(f.calls.every((call) => !call.args.includes('authenticated-automation')));
  assert.ok(!JSON.stringify(result).includes(SECRET));
});

test('google.de: the business panel website counts when its page carries the name token', async () => {
  const f = fixture('google-de-authenticated.js', { company: 'Example Oberflaechenchemie GmbH & Co. KG', country: 'DE' }, {
    responses: [{ status: 200, body: serp({
      knowledge: { name: 'EXAMPLE OBERFLAECHENCHEMIE GmbH & Co. KG', site: 'https://www.example.org/' },
      organic: [{ link: 'https://portal.test/example-als-arbeitgeber', title: 'Example Oberflaechenchemie GmbH & Co. KG', description: '' }],
    }) }],
    ctox: companyPage('', (url) => (url.includes('example.org') ? 'EXAMPLE, Ihre Spezialisten' : 'Example Oberflaechenchemie GmbH & Co. KG')),
  });
  const result = await f.run();
  const domain = result.records.find((r) => r.field === 'firma_domain');
  assert.equal(domain.value, 'example.org');
});

test('google.de: missing Bright Data key is authorization_required without a request', async () => {
  const f = fixture('google-de-authenticated.js', { company: 'Example GmbH', country: 'DE' }, { token: '' });
  const result = await f.run();
  assert.equal(result.failure_mode, 'authorization_required');
  assert.equal(f.requests.length, 0);
});

test('google.de: a rejected key is authorization_required and not retried', async () => {
  const f = fixture('google-de-authenticated.js', { company: 'Example GmbH', country: 'DE' }, {
    responses: [{ status: 401, body: 'invalid token' }],
  });
  const result = await f.run();
  assert.equal(result.failure_mode, 'authorization_required');
  assert.equal(f.requests.length, 1);
});

test('google.de: empty 200 answers are retried with a different URL, then temporary_unreachable', async () => {
  const f = fixture('google-de-authenticated.js', { company: 'Example GmbH', country: 'DE' }, {
    responses: [{ status: 200, body: '' }, { status: 200, body: 'This query recently failed' }, { status: 200, body: '' }],
  });
  const result = await f.run();
  assert.equal(result.failure_mode, 'temporary_unreachable');
  assert.deepEqual(result.records, []);
  assert.equal(f.requests.length, 3);
  assert.equal(new Set(f.requests.map((r) => r.body.url)).size, 3);
});

test('google.de: results from a non-Google engine are rejected', async () => {
  const foreign = { status: 200, body: { general: { search_engine: 'bing' }, organic: [{ link: 'https://www.example.de/', title: 'Example GmbH' }] } };
  const f = fixture('google-de-authenticated.js', { company: 'Example GmbH', country: 'DE' }, { responses: [foreign, foreign, foreign] });
  const result = await f.run();
  assert.equal(result.failure_mode, 'temporary_unreachable');
  assert.deepEqual(result.records, []);
});

test('google.de: a company-page access challenge stays blocked, never success', async () => {
  const f = fixture('google-de-authenticated.js', { company: 'Example GmbH', country: 'DE' }, {
    responses: [{ status: 200, body: serp({ organic: [{ link: 'https://www.example.de/', title: 'Example GmbH', description: '' }] }) }],
    ctox: (args) => (args[1] === 'browser-automation'
      ? { ok: true, detection: { markers: ['captcha'] }, result: { url: 'https://www.example.de/', page_text_excerpt: 'Verify you are human' } }
      : { ok: false }),
  });
  const result = await f.run();
  assert.equal(result.failure_mode, 'blocked');
  assert.deepEqual(result.records, []);
});

test('google.de: no matching result is portal_drift and never invents a company', async () => {
  const f = fixture('google-de-authenticated.js', { company: 'Example GmbH', country: 'DE' }, {
    responses: [{ status: 200, body: serp({ organic: [{ link: 'https://other.de/', title: 'Other AG', description: '' }] }) }],
  });
  const result = await f.run();
  assert.equal(result.failure_mode, 'portal_drift');
  assert.deepEqual(result.records, []);
});

test('google.de: missing company is reported before any provider call', async () => {
  const f = fixture('google-de-authenticated.js', {});
  const result = await f.run();
  assert.equal(result.failure_mode, 'portal_drift');
  assert.equal(f.requests.length, 0);
  assert.equal(f.calls.length, 0);
});

// --- google.de: person mode --------------------------------------------------

test('google.de: title_groups run one OR-combined LinkedIn search per group in parallel', async () => {
  const hitsFor = (query) => (query.includes('Einkauf')
    ? [
      { link: 'https://de.linkedin.com/in/Erika-Muster-1a2b?trk=x', title: 'Erika Muster – Leiterin Einkauf – EXAMPLE | LinkedIn', description: 'Leiterin Einkauf bei EXAMPLE Chemie' },
      { link: 'https://de.linkedin.com/in/fremd-person', title: 'Fremd Person – Einkauf – Andere AG | LinkedIn', description: 'Andere AG' },
      { link: 'https://www.example-chemie.de/team', title: 'Team EXAMPLE', description: '' },
    ]
    : [{ link: 'https://www.linkedin.com/in/max-beispiel', title: 'Max Beispiel - Geschäftsführer bei EXAMPLE ... | LinkedIn', description: '' }]);
  const answer = (body) => ({ status: 200, body: serp({ organic: hitsFor(new URL(body.url).searchParams.get('q')) }) });
  const f = fixture('google-de-authenticated.js', {
    company: 'Example Chemie GmbH & Co. KG', country: 'DE',
    title_groups: [['Leiter Einkauf', 'CPO'], ['Geschäftsführer', 'CEO']],
  }, { responses: [answer, answer] });
  const result = await f.run();
  const queries = f.requests.map((r) => new URL(r.body.url).searchParams.get('q')).sort();
  assert.deepEqual(queries, [
    'linkedin "Example Chemie" ("Leiter Einkauf" OR CPO)',
    'linkedin "Example Chemie" (Geschäftsführer OR CEO)',
  ]);
  const people = result.records.filter((r) => r.field === 'person_linkedin').map((r) => r.value).sort();
  assert.deepEqual(people, ['https://www.linkedin.com/in/erika-muster-1a2b', 'https://www.linkedin.com/in/max-beispiel']);
  const erika = result.records.filter((r) => r.source_url === 'https://www.linkedin.com/in/erika-muster-1a2b');
  assert.deepEqual(Object.fromEntries(erika.map((r) => [r.field, r.value])), {
    person_vorname: 'Erika',
    person_nachname: 'Muster',
    person_position: 'Leiterin Einkauf – EXAMPLE',
    person_linkedin: 'https://www.linkedin.com/in/erika-muster-1a2b',
  });
  assert.ok(erika[0].note.includes('Leiterin Einkauf bei EXAMPLE Chemie'));
  assert.ok(!result.records.some((r) => r.value === 'Fremd'));
});

test('google.de: person search without any matching profile is portal_drift', async () => {
  const f = fixture('google-de-authenticated.js', { company: 'Example GmbH', country: 'DE', titles: ['CFO'] }, {
    responses: [{ status: 200, body: serp({ organic: [] }) }],
  });
  const result = await f.run();
  assert.equal(result.failure_mode, 'portal_drift');
  assert.equal(f.requests.length, 1);
  assert.equal(new URL(f.requests[0].body.url).searchParams.get('q'), 'linkedin "Example" CFO');
});

test('google.de: a free query also runs as a LinkedIn site search on the distinctive name word', async () => {
  const answer = (body) => {
    const q = new URL(body.url).searchParams.get('q');
    return { status: 200, body: serp({ organic: q.startsWith('site:')
      ? [{ link: 'https://de.linkedin.com/in/nina-beispiel', title: 'Nina Beispiel – Head of Supply Chain Germany – Budenheim | LinkedIn', description: '' }]
      : [{ link: 'https://www.example.org/news', title: 'Chemische Fabrik news', description: '' }] }) };
  };
  const f = fixture('google-de-authenticated.js', {
    company: 'Chemische Fabrik Budenheim KG', country: 'DE',
    query: 'linkedin Chemische Fabrik Budenheim KG Supply Chain Leiter Logistik',
  }, { responses: [answer, answer] });
  const result = await f.run();
  const queries = f.requests.map((r) => new URL(r.body.url).searchParams.get('q')).sort();
  assert.deepEqual(queries, [
    'linkedin Chemische Fabrik Budenheim KG Supply Chain Leiter Logistik',
    'site:linkedin.com/in "Budenheim" (supply OR chain OR logistik)',
  ]);
  assert.deepEqual(result.records.filter((r) => r.field === 'person_linkedin').map((r) => r.value),
    ['https://www.linkedin.com/in/nina-beispiel']);
});

// --- maps.google.com ---------------------------------------------------------

test('maps.google.com: business panel yields address, phone and domain on the Maps search URL', async () => {
  const route = (body) => {
    const local = new URL(body.url).searchParams.get('tbm') === 'lcl';
    return { status: 200, body: local ? serp({ snack_pack: [] }) : serp({ knowledge: {
      name: 'EXAMPLE CHEMIE GmbH', address: 'Werkstraße 1, 12345 Musterstadt', phone: '01234 5678',
      site: 'https://www.example-chemie.de/', open_hours: [{ day: 'Montag', hours: '08:00–16:00' }],
    } }) };
  };
  const f = fixture('google-maps-authenticated.js', { company: 'Example Chemie GmbH', country: 'DE' }, { responses: [route, route] });
  const result = await f.run();
  const fields = Object.fromEntries(result.records.map((r) => [r.field, r.value]));
  assert.deepEqual(fields, {
    firma_name: 'EXAMPLE CHEMIE GmbH', firma_anschrift: 'Werkstraße 1', firma_plz: '12345',
    firma_telefon: '01234 5678', firma_domain: 'example-chemie.de',
  });
  for (const record of result.records) assert.ok(record.source_url.startsWith('https://www.google.com/maps/search/?api=1'));
  assert.ok(result.records[0].note.includes('Montag 08:00–16:00'));
  assert.equal(f.requests.length, 2);
  assert.ok(f.calls.every((call) => !call.args.includes('authenticated-automation')));
});

test('maps.google.com: local results fill in when the web search shows no panel', async () => {
  const route = (body) => (new URL(body.url).searchParams.get('tbm') === 'lcl'
    ? { status: 200, body: serp({ snack_pack: [{ name: 'Example Services', phone: '07764 1234', address: 'Dorf',
      maps_link: 'https://www.google.de/maps/dir//Example+Services,+Gewerbepark+1,+4751+Dorf+an+der+Pram/data=!4m6' }] }) }
    : { status: 200, body: serp() });
  const f = fixture('google-maps-authenticated.js', { company: 'Example Services GmbH', country: 'AT' }, { responses: [route, route] });
  const result = await f.run();
  const fields = Object.fromEntries(result.records.map((r) => [r.field, r.value]));
  assert.equal(fields.firma_anschrift, 'Gewerbepark 1');
  assert.equal(fields.firma_plz, '4751');
  assert.equal(fields.firma_telefon, '07764 1234');
});

test('maps.google.com: no matching place is the old no-exact-result failure', async () => {
  const f = fixture('google-maps-authenticated.js', { company: 'Example GmbH', country: 'DE' }, {
    responses: [{ status: 200, body: serp({ knowledge: { name: 'Other AG' } }) }, { status: 200, body: serp({ snack_pack: [] }) }],
  });
  const result = await f.run();
  assert.equal(result.failure_mode, 'temporary_unreachable');
  assert.equal(result.detail, 'no exact Google Maps company result');
});

test('maps.google.com: rejected key is authorization_required', async () => {
  const f = fixture('google-maps-authenticated.js', { company: 'Example GmbH', country: 'DE' }, {
    responses: [{ status: 403, body: '' }, { status: 403, body: '' }],
  });
  assert.equal((await f.run()).failure_mode, 'authorization_required');
});

test('Maps never substitutes a previous company when input is missing', async () => {
  const f = fixture('google-maps-authenticated.js', {});
  const result = await f.run();
  assert.equal(result.failure_mode, 'invalid_input');
  assert.equal(f.calls.length, 0);
  assert.equal(f.requests.length, 0);
});
