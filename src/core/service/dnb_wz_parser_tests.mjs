import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const rust = readFileSync(new URL('./business_os.rs', import.meta.url), 'utf8');
const parser = rust.match(/const DNB_BROWSER_WZ_PARSER: &str = r#"([\s\S]*?)"#;/)?.[1];
assert.ok(parser, 'test the actual native-embedded parser');
const fixture = JSON.parse(readFileSync(new URL('./fixtures/dnb-wz-captured.json', import.meta.url)));
const scope = vm.createContext({ URL });
vm.runInContext(parser + '\nglobalThis.parse = parseDnbWzEvidence;', scope);
const snapshot = { url: fixture.url, title: fixture.company, headings: [fixture.company], text: fixture.text };
const parse = (overrides = {}, company = fixture.company, expectedUrl = fixture.url) =>
  JSON.parse(JSON.stringify(scope.parse(company, { ...snapshot, ...overrides }, expectedUrl)));
const cases = [];
const check = (name, fn) => { fn(); cases.push(name); };
check('saved NACE 2014 plus WZ 20140 returns the explicit WZ and literal quote', () => {
  const wz = parse();
  assert.equal(wz.value, '20140');
  assert.equal(wz.source_url, fixture.url);
  assert.equal(wz.source_quote, 'WZ 2008 (DE) | 20140 - Manufacture of other organic basic chemicals');
  assert.ok(fixture.text.includes(wz.source_quote));
  assert.ok(wz.note.includes(wz.source_quote));
});
check('NACE, SIC, NOGA and OENACE are not WZ substitutes', () => {
  for (const text of ['NACE REV 2 | 2014', 'US SIC 1987 | 2869',
    'NOGA 2008 | 20140', 'ÖNACE 2008 | 20140']) assert.equal(parse({ text }), null);
});
check('classification year, incomplete code and separated other scheme are rejected', () => {
  for (const text of ['WZ 2008', 'WZ 2008 (DE) | 2014', 'WZ 2008\nNACE REV 2\n20140',
    'WZ 2003 | 20140', 'WZ 2008 | 201400']) assert.equal(parse({ text }), null);
});
check('conflicting explicit WZ entries are rejected; repeated same evidence is permitted', () => {
  assert.equal(parse({ text: fixture.text + '\nWZ 2008 | 20150 - Other activity' }), null);
  assert.equal(parse({ text: fixture.text + '\nWZ 2008 | 20140 - Same activity' }).value, '20140');
});
check('line-separated and dotted subclasses retain the literal observed value', () => {
  for (const code of ['20140', '20.14.0']) {
    const text = 'WZ 2008 (DE)\n' + code + ' - Organic chemicals';
    const result = parse({ text });
    assert.equal(result.value, code);
    assert.ok(text.includes(result.source_quote));
  }
});
check('requested company in body or title cannot rescue a different company heading', () => {
  assert.equal(parse({ headings: ['Another Chemicals GmbH'], text: fixture.company + fixture.text }), null);
  assert.equal(parse({}, 'Carbosulf Chemische Werke Holding GmbH'), null);
  assert.equal(parse({ headings: [], title: 'Another Chemicals GmbH' }), null);
  assert.equal(parse({ headings: [], title: fixture.company + ' | D&B Hoovers' }).value, '20140');
});
check('same company name on a foreign profile or origin is rejected', () => {
  for (const url of [fixture.url.replace('3e55b38a', '4e55b38a'),
    fixture.url.replace('app.dnbhoovers.com', 'evil.test'),
    fixture.url.replace('https:', 'http:'),
    fixture.url.replace('https://', 'https://user:pass@')]) assert.equal(parse({ url }), null);
});

// Execute the actual rendered Rust capture template with a deterministic page
// fixture. This checks the caller, not only the helper; it performs no browser IO.
const template = rust.match(/fn build_web_stack_authenticated_source_capture\([\s\S]*?r#"([\s\S]*?)"#,/)[1];
function render() {
  const args = [JSON.stringify('dnbhoovers.com'), JSON.stringify(fixture.company),
    JSON.stringify('DE'), parser];
  const result = template.replace(/\{\{|\}\}|\{\}/g, (token) =>
    token === '{}' ? args.shift() : token === '{{' ? '{' : '}');
  assert.equal(args.length, 0);
  return result;
}
async function capture(detail) {
  let reads = 0;
  const hitUrl = fixture.url.split('/report/')[0];
  const page = {
    url: () => reads ? detail.url : 'https://app.dnbhoovers.com/search',
    locator: () => ({
      first() { return this; }, filter() { return this; },
      count: async () => 0, waitFor: async () => {},
    }),
    waitForLoadState: async () => {}, waitForTimeout: async () => {},
    goto: async (url) => { assert.equal(url, hitUrl); },
    evaluate: async () => {
      reads++;
      if (reads === 1) return { title: fixture.company, text: fixture.company,
        links: [{ url: hitUrl, text: fixture.company,
          context: fixture.company + ' Folgen Köln, Deutschland +49-22174960 Umsatz EUR: 50,87M Beschäftigte (Gesamt): 31' }] };
      if (reads === 2) return 10000;
      return detail;
    },
  };
  const context = vm.createContext({ URL, page });
  return JSON.parse(JSON.stringify(await vm.runInContext('(async () => {' + render() + '\n})()', context)));
}
const good = await capture(snapshot);
assert.equal(good.records.find((record) => record.field === 'wz_code').value, '20140');
assert.ok(good.records.find((record) => record.field === 'wz_code').source_quote);
const naceOnly = await capture({ ...snapshot, text: 'NACE REV 2 | 2014' });
assert.equal(naceOnly.records.some((record) => record.field === 'wz_code'), false);
assert.deepEqual(good.records.filter((record) => !['wz_code', 'branche_codes_rohtext'].includes(record.field)),
  naceOnly.records.filter((record) => !['wz_code', 'branche_codes_rohtext'].includes(record.field)));
const foreign = await capture({ ...snapshot, headings: ['Another Chemicals GmbH'] });
assert.equal(foreign.records.some((record) => ['wz_code', 'branche_codes_rohtext'].includes(record.field)), false);
cases.push('actual rendered native capture preserves other fields and refuses NACE and foreign detail');
console.log(JSON.stringify({ ok: true, tests: cases.length, cases, scope: 'saved fixture; no live provider/browser IO' }));
