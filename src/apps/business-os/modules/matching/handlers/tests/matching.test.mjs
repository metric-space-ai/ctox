import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { makeCtx, makeCommand, rejectsWithCode } from './_ctx.mjs';
import { hexSha256, shortHash, slug, normalizeLines, lines, decodeBase64, nowIso } from '../_lib/text.js';
import { dimensionScore, contentOverlapScore, totalMatchScore, buildMatchItems, snippetForDimension } from '../_lib/score.js';
import { parseCandidateText, cleanRoleLine, filenameName, containsSkill } from '../_lib/candidate.js';
import match from '../matching.match.js';
import parseObject from '../matching.source.parse_object.js';

test('hash/slug/base64 helpers match Rust semantics', () => {
  assert.equal(hexSha256('Größe'), createHash('sha256').update('Größe').digest('hex'));
  assert.equal(shortHash('abc'), createHash('sha256').update('abc').digest('base64url').slice(0, 10));
  assert.equal(slug('Jürgen  Groß!'), 'juergen-gross');
  assert.equal(slug('!!!'), `record-${shortHash('!!!')}`);
  assert.deepEqual([...decodeBase64('aGk=')], [104, 105]);
  assert.equal(decodeBase64('a$b='), null);
  assert.deepEqual(lines('a\r\nb\n'), ['a', 'b']);
  assert.equal(normalizeLines(' a   b \n\n c '), 'a b\nc');
  assert.equal(nowIso(Date.UTC(2026, 9, 15, 11, 22, 33, 456)), '2026-10-15T11:22:33Z');
});

test('dimension scoring: keyword path, overlap fallback, clamps', () => {
  assert.equal(dimensionScore('skill', 'python sql cad', 'Python only'), 0.28 + (1 / 3) * 0.68);
  assert.equal(dimensionScore('skill', 'python', 'python'), 0.96);
  assert.equal(contentOverlapScore('und der', 'x'), 0.1);
  assert.equal(dimensionScore('language', 'Floristik Gestaltung', 'Floristik'), 0.1 + 0.5 * 0.8);
  const items = buildMatchItems('s', 'r', 'o', 'Anforderungen: python\nTeam', 'Skills: python\nTeam', 'T');
  assert.equal(items.length, 5);
  assert.equal(items[0].id, 's|r|o|REQ-1');
  assert.equal(items[0].matchLevel, 'full');
  assert.equal(items[0].matchScoreKey, 96);
  assert.equal(items[0].jobSnippet, 'Anforderungen: python');
  assert.equal(totalMatchScore([{ matchScore: 0.5 }, { matchScore: 0.505 }]), 50);
  assert.equal(snippetForDimension('x'.repeat(300), 'skill'), `${'x'.repeat(240)}...`);
});

test('candidate parse extracts fields like importer.rs', () => {
  const cv = [
    'Lebenslauf Max Mustermann',
    'E-Mail: max.mustermann@example.de',
    'Telefon: +49 170 1234567',
    'Wohnort: 70173 Stuttgart',
    '03/2019 - heute',
    'Entwicklungsingenieur Fahrerassistenzsysteme bei Bosch',
    'Master Maschinenbau, Deutsch, Englisch, Führerschein Klasse B',
    'Skills: Python, C++, CATIA, Projektleitung',
  ].join('\n');
  const c = parseCandidateText('Lebenslauf_Mustermann_Max.pdf', cv);
  assert.equal(c.name, 'Max Mustermann');
  assert.equal(c.first_name, 'Max');
  assert.equal(c.last_name, 'Mustermann');
  assert.equal(c.email, 'max.mustermann@example.de');
  assert.equal(c.phone, '+49 170 1234567');
  assert.equal(c.region, '70173 Stuttgart');
  assert.equal(c.current_role, 'Entwicklungsingenieur Fahrerassistenzsysteme bei Bosch');
  assert.equal(c.highest_degree, 'Master');
  assert.deepEqual(c.driver_license, ['B']);
  assert.deepEqual(c.languages, [{ code: 'de', level: '' }, { code: 'en', level: '' }]);
  assert.deepEqual(c.skills, ['Python', 'C++', 'CATIA', 'Projektleitung', 'Maschinenbau', 'Fahrerassistenzsysteme']);
  assert.equal(c.leadership, 'Hinweise auf Koordination, Teamarbeit oder fachliche Leitung im CV.');
  assert.equal(c.confidence, 0.78);
  assert.equal(parseCandidateText('x.pdf', '').confidence, 0.25);
  assert.equal(filenameName('Lebenslauf_Mustermann_Max.pdf'), 'Max Mustermann');
  assert.equal(cleanRoleLine('Seit 2019 Projektleiter |'), 'Projektleiter');
  assert.equal(containsSkill('Kenntnisse in C und Java', 'C'), true);
  assert.equal(containsSkill('C#', 'C'), false);
});

function matchingWorld() {
  const world = makeCtx({
    now: Date.UTC(2026, 9, 15, 10, 0, 0),
    collections: {
      matching_requirements: [{ id: 'req-1', title: 'Ingenieur Automatisierung', sourceName: 'ACME', objectRequirements: ['Python', 'SQL', 'Studium Maschinenbau'], rawText: 'Team, Deutsch' }],
      matching_objects: [
        { id: 'obj-1', name: 'Max Mustermann', skills: ['Python', 'Maschinenbau'], languages: [{ code: 'de' }], rawText: 'Studium Maschinenbau, Team' },
        { id: 'base__alt', name: 'Alt' },
        { id: 'other__keep', name: 'Keep' },
      ],
    },
    caps: { doc: { extract: (args) => {
      // Kernform §16.32: nur {blob_id:{ref}, kind, ocr}; der Text kommt aus dem vorher angelegten files-Blob.
      assert.deepEqual(Object.keys(args).sort(), ['blob_id', 'kind', 'ocr']);
      assert.deepEqual(Object.keys(args.blob_id), ['ref']);
      return { text: world.state.files.get(args.blob_id.ref).bytes.toString('utf8') };
    } } },
  });
  return world;
}

test('matching.match writes a scored result', async () => {
  const { ctx, state } = matchingWorld();
  const out = await match(makeCommand('matching.match', { request: { requirement_id: 'req-1', object_id: 'obj-1' } }, { client_context: { sourceId: 'src-1' } }), ctx);
  assert.deepEqual(out.record_ids, ['src-1|req-1|obj-1']);
  const env = state.collections.get('matching_results').get('src-1|req-1|obj-1');
  assert.deepEqual([env.kind, env.status, typeof env.updated_at_ms, env.title], ['match', 'prematch', 'number', 'match src-1|req-1|obj-1']);
  const doc = env.data;
  assert.equal(doc.status, 'prematch');
  assert.equal(doc.items.length, 5);
  assert.equal(doc.score, out.score);
  assert.equal(doc.requirementTitle, 'Ingenieur Automatisierung');
  assert.equal(doc.createdAt, '2026-10-15T10:00:00Z');
  assert.ok(doc.score > 0 && doc.score <= 100);
  await rejectsWithCode(match(makeCommand('matching.match', { request: { requirement_id: 'req-1' } }), ctx), 'invalid');
  await rejectsWithCode(match(makeCommand('matching.match', { request: { requirement_id: 'nope', object_id: 'obj-1' } }), ctx), 'not_found');
});

test('matching.match reads UI-written envelopes (data.requirement / data.object)', async () => {
  const { ctx, state } = makeCtx({
    collections: {
      matching_requirements: [{ id: 'r', kind: 'requirement', title: 'R', status: 'active', updated_at_ms: 1, data: { requirement: { id: 'r', title: 'Python Entwickler', objectRequirements: 'python sql' } } }],
      matching_objects: [{ id: 'o', kind: 'object', title: 'O', status: 'active', updated_at_ms: 1, data: { id: 'o', name: 'Eva', skills: ['Python', 'SQL'] } }],
    },
  });
  const out = await match(makeCommand('matching.match', {}, { client_context: { requirementId: 'r', objectId: 'o' } }), ctx);
  const doc = state.collections.get('matching_results').get('|r|o').data;
  assert.equal(doc.requirementTitle, 'Python Entwickler');
  assert.equal(doc.items[0].matchLevel, 'full');
  assert.equal(out.record_ids[0], '|r|o');
});

test('matching.source.parse_object replaces previous objects of the same base record', async () => {
  const { ctx, state } = matchingWorld();
  const text = 'Erika Musterfrau\nBachelor Elektrotechnik\nSQL';
  const out = await parseObject(makeCommand('matching.source.parse_object', {
    input: { files: [{ name: 'cv.pdf', type: 'application/pdf', base64: Buffer.from(text).toString('base64') }] },
  }, { record_id: 'base' }), ctx);
  assert.deepEqual(out.record_ids, ['base__erika-musterfrau']);
  const objects = state.collections.get('matching_objects');
  assert.equal(objects.has('base__alt'), false);
  assert.equal(objects.has('other__keep'), true);
  const doc = objects.get('base__erika-musterfrau').data;
  assert.equal(doc.highestDegree, 'Bachelor');
  assert.deepEqual(doc.skills, ['SQL', 'Elektrotechnik']);
  assert.equal(doc.documents[0].size, Buffer.byteLength(text));
  assert.equal(doc.documents[0].meta.sha256, createHash('sha256').update(text).digest('hex'));
  assert.equal(out.records[0].raw_text_length, Buffer.byteLength(text));
  await rejectsWithCode(parseObject(makeCommand('matching.source.parse_object', { input: { files: [] } }), ctx), 'invalid');
  await rejectsWithCode(parseObject(makeCommand('matching.source.parse_object', { input: { files: [{ name: 'x.pdf', base64: '%%%' }] } }), ctx), 'invalid');
});

test('parse_object tolerates extraction failure with empty text', async () => {
  const { ctx } = makeCtx({ caps: { doc: { extract: () => { throw { code: 'unavailable', message: 'no pdf' }; } } } });
  const out = await parseObject(makeCommand('matching.source.parse_object', { input: { files: [{ name: 'Lebenslauf_Doe_Jane.pdf', base64: 'aGk=' }] } }, { record_id: 'b' }), ctx);
  assert.deepEqual(out.record_ids, ['b__jane-doe']);
  assert.equal(out.records[0].raw_text_length, 0);
});

