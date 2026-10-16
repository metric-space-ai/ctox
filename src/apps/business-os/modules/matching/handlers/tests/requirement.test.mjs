import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, makeCommand, rejectsWithCode } from './_ctx.mjs';
import { splitJobSections, bulletize, inferFachlevel, findJobposting, addressText, originUrl } from '../_lib/job.js';
import { shortHash } from '../_lib/text.js';
import parseRequirement from '../matching.source.parse_requirement.js';

const RAW = [
  'Wir sind ACME.',
  'Stellenbeschreibung:',
  '- Planung von Automatisierungsprojekten',
  'Qualifikationen:',
  '- Studium Elektrotechnik oder vergleichbar',
  'Wir bieten Ihnen:',
  '- Mobiles Arbeiten nach Absprache',
  'Zusätzliche Informationen:',
  'Bewerbung bis Ende Oktober',
].join('\n');

test('job section split, bullets, fachlevel', () => {
  const [company, role, req, benefits, closing] = splitJobSections(RAW);
  assert.equal(company, 'Wir sind ACME.');
  assert.equal(role, '- Planung von Automatisierungsprojekten');
  assert.equal(req, 'Qualifikationen:\n- Studium Elektrotechnik oder vergleichbar\nWir bieten Ihnen:\n- Mobiles Arbeiten nach Absprache');
  assert.equal(benefits, '- Mobiles Arbeiten nach Absprache\nZusätzliche Informationen:\nBewerbung bis Ende Oktober');
  assert.equal(closing, 'Bewerbung bis Ende Oktober');
  assert.deepEqual(bulletize('- kurz\n• Lange Aufgabe hier\n* x'), ['Lange Aufgabe hier']);
  assert.equal(inferFachlevel('Projekt', ''), 3);
  assert.equal(inferFachlevel('Meister', ''), 2);
  assert.equal(inferFachlevel('', ''), 1);
});

test('json-ld helpers', () => {
  const job = { '@type': 'JobPosting', title: 'T' };
  assert.equal(findJobposting({ '@graph': [{ '@type': 'Org' }, job] }), job);
  assert.equal(addressText({ addressLocality: 'Stuttgart', postalCode: '70173' }), 'Stuttgart, 70173, Deutschland');
  assert.equal(originUrl('https://jobs.example.org/a?id=1'), 'https://jobs.example.org');
});

// Kernform: doc.html({blob_id:{ref}, selector, attribute?, limit?}) → {matches}. Die Attrappe beantwortet je
// Selektor aus `dom` (Schlüssel wie JOB_HTML_QUERIES); das JSON-LD-Fragment liefert RAW über selector 'body'.
const SELECTOR_KEY = {
  'script[type="application/ld+json"]': 'ld_json', 'meta[property="og:title"]': 'og_title', title: 'title',
  '.jobdescription': 'jobdescription', body: 'body', '.jobGeoLocation': 'geo',
  '[data-careersite-propertyid="city"]': 'city', '[itemprop="hiringOrganization"]': 'org', '[itemprop="datePosted"]': 'date',
};
function world(dom) {
  const env = makeCtx({
    now: Date.UTC(2026, 9, 15, 10, 0, 0),
    caps: {
      web: { fetch: (args) => {
        assert.deepEqual(Object.keys(args), ['url']);
        return { blob_id: { ref: 'f'.repeat(64) }, sha256: 'f'.repeat(64), url: args.url, title: 'Seitentitel' };
      } },
      doc: { html: (args) => {
        assert.deepEqual(Object.keys(args.blob_id), ['ref']);
        for (const k of Object.keys(args)) assert.ok(['blob_id', 'selector', 'attribute', 'limit'].includes(k), k);
        const stored = env.state.files.get(args.blob_id.ref);
        if (stored && stored.meta.name === 'fragment.html') return { matches: [RAW] };
        const key = SELECTOR_KEY[args.selector];
        const value = dom[key];
        if (key === 'org' || key === 'date') return { matches: args.attribute ? [null] : (value == null ? [] : [value]) };
        if (Array.isArray(value)) return { matches: value };
        return { matches: value == null ? [] : [value] };
      } },
    },
  });
  return env;
}

test('parse_requirement builds source, requirement and requirementSource from JSON-LD', async () => {
  const ld = JSON.stringify({
    '@type': 'JobPosting',
    title: ' Ingenieur  Automatisierung - Bosch Stellenportal',
    description: '<p>…</p>',
    hiringOrganization: { name: 'Robert Bosch GmbH', sameAs: 'https://bosch.com', logo: 'https://bosch.com/l.png' },
    jobLocation: [{ address: { addressLocality: 'Stuttgart', postalCode: '70173', streetAddress: 'Weg 1' } }],
    identifier: { value: 'REF-9' },
    datePosted: '2026-10-01T08:00:00Z',
  });
  const { ctx, state } = world({ ld_json: ['not json', ld] });
  const out = await parseRequirement(makeCommand('matching.source.parse_requirement', { input: { url: 'https://jobs.bosch.de/x?id=1' } }, { record_id: 'req-7' }), ctx);
  const sourceId = `source_${shortHash('Robert Bosch GmbH:Stuttgart, 70173, Deutschland')}`;
  assert.deepEqual(out.record_ids, [sourceId, 'req-7', 'req-7::source']);
  const reqs = state.collections.get('matching_requirements');
  const req = reqs.get('req-7').data;
  assert.equal(reqs.get('req-7').title, 'Ingenieur Automatisierung');
  assert.equal(req.title, 'Ingenieur Automatisierung');
  assert.equal(req.internalReferenceId, 'REF-9');
  assert.equal(req.fachlevelClass, 3);
  assert.equal(req.remote, true);
  assert.equal(req.location, 'Stuttgart, 70173, Deutschland');
  assert.equal(reqs.get(sourceId).data.locations[0].address, 'Weg 1');
  assert.equal(reqs.get('req-7::source').data.parsingMeta.httpStatus, 200);
  assert.equal(reqs.get('req-7::source').data.source, 'Bosch Stellenportal');
  assert.equal(reqs.get('req-7::source').data.rawHtmlSha256, 'f'.repeat(64));
  assert.ok(state.capCalls.filter((c) => c.path === 'doc.html').every((c) => c.args[0].blob_id.ref === 'f'.repeat(64)
    || state.files.get(c.args[0].blob_id.ref)?.meta.name === 'fragment.html'));
});

test('parse_requirement with inline html stores it as a file and queries the DOM over its ref', async () => {
  const { ctx, state } = world({ ld_json: [], og_title: 'Monteur (m/w/d)', title: 'x', body: RAW, geo: null, city: null, org: 'ACME AG' });
  await parseRequirement(makeCommand('matching.source.parse_requirement', { input: { html: '<html><title>x</title></html>' } }, { id: 'cmd-h' }), ctx);
  const [file] = [...state.files.values()];
  assert.equal(file.meta.mime, 'text/html');
  const req = state.collections.get('matching_requirements').get('cmd-h').data;
  assert.equal(req.title, 'Monteur (m/w/d)');
});

test('parse_requirement with inline text uses DOM fallbacks and no fetch', async () => {
  const { ctx, state } = world({ ld_json: [], og_title: null, title: '\nStellenanzeige\n', body: RAW, geo: null, city: null, org: 'ACME AG' });
  const out = await parseRequirement(makeCommand('matching.source.parse_requirement', { input: { text: 'Wir sind ACME.' } }, { id: 'cmd-x' }), ctx);
  assert.equal(out.record_ids[1], 'cmd-x');
  assert.equal(state.capCalls.some((c) => c.path === 'web.fetch'), false);
  const req = state.collections.get('matching_requirements').get('cmd-x').data;
  assert.equal(req.title, 'Stellenanzeige');
  assert.equal(req.sourceName, 'ACME AG');
  await rejectsWithCode(parseRequirement(makeCommand('matching.source.parse_requirement', { input: {} }), ctx), 'invalid');
});
