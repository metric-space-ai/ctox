import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

// Execute the production JS embedded in Rust; no provider/browser/network call.
const rust = readFileSync(process.argv[2] || new URL('../src/sources/xing.rs', import.meta.url), 'utf8');
const raw = (name) => {
  const match = rust.match(new RegExp(`const ${name}: &str = r#"([\\s\\S]*?)"#;`));
  assert.ok(match, `${name} must exist`);
  return match[1];
};
const parser = raw('XING_BROWSER_RECORD_PARSER');
const company = 'Carbosulf Chemische Werke GmbH';
const profile = (name, contextLines) => ({ url: `https://www.xing.com/profile/${name}`, text: name.replaceAll('_', ' '), contextLines });
function parse(query, links, companies = []) {
  return JSON.parse(vm.runInNewContext(`${parser}; JSON.stringify(parseXingRecords(company, companies, members))`, {
    URL, company: query,
    companies: { links: companies, sourceUrl: `https://www.xing.com/search/companies?keywords=${encodeURIComponent(query)}` },
    members: { links, sourceUrl: `https://www.xing.com/search/members?keywords=${encodeURIComponent(query)}` },
  }, { timeout: 1000 }));
}
const good = profile('Anna_Muster', ['Anna Muster', 'Leiterin Einkauf', company, 'Köln, Deutschland']);
const foreign = profile('Ford_Mitarbeiter', ['Ford Mitarbeiter', 'Director', 'Ford-Werke Köln']);
const records = parse(company, [good, foreign]);
assert.equal(records.length, 4);
assert.ok(records.every((record) => record.source_url === good.url));
assert.ok(records.every((record) => record.note === `XING member result current employer: "${company}"`));
assert.equal(records.find((record) => record.field === 'person_funktion').value, 'Leiterin Einkauf');
assert.ok(!records.some((record) => record.field === 'firma_name'), 'Member-query echo must never become a company fact');
assert.equal(parse(company, [profile('Anna_Muster', ['Anna Muster', 'Ehemaliges Unternehmen', company])]).length, 0);
assert.equal(parse(company, [profile('Anna_Muster', ['Anna Muster', 'Ehemaliges Unternehmen', 'Director', 'Kontakte: 284', company])]).length, 0, 'A former-employment heading persists across intervening role/metadata lines');
assert.equal(parse(company, [profile('Anna_Muster', ['Anna Muster', 'Ehemaliges Unternehmen', 'Ford-Werke Köln', 'Derzeitiges Unternehmen:', 'Leiterin Einkauf', company])]).length, 4, 'An explicit current heading starts a new employment section');
assert.equal(parse('Josef Göbel GmbH', [profile('Josef_Goebel', ['Josef Göbel', 'Senior Software Engineer', 'Josef Göbel'])]).length, 0);
const neighbor = profile('Ford_Mitarbeiter', ['Ford Mitarbeiter', 'Director', 'Ford-Werke Köln', ...good.contextLines]);
assert.ok(parse(company, [neighbor, good]).every((record) => record.source_url === good.url), 'Neighbor card must not lend its employer');
const actualFirm = { url: 'https://www.xing.com/pages/carbosulf', text: company, contextLines: [company] };
assert.deepEqual(parse(company, [], [actualFirm]).map((record) => [record.field, record.value]), [['firma_name', company]]);
assert.equal(parse(company, [], [{ ...actualFirm, text: 'Ford-Werke', contextLines: ['Ford-Werke Köln'] }]).length, 0);

const template = raw('XING_BROWSER_CAPTURE_TEMPLATE')
  .replace('__COMPANY_JSON__', JSON.stringify(company)).replace('__COUNTRY_JSON__', '"DE"')
  .replace('__MEMBER_SEARCH_URL_JSON__', '"https://www.xing.com/search/members"')
  .replace('__COMPANY_SEARCH_URL_JSON__', '"https://www.xing.com/search/companies"')
  .replace('__RECORD_PARSER__', parser);
async function capture({ memberLinks = [], mode = 'ok', dom = false } = {}) {
  let currentUrl = 'https://www.xing.com/';
  let kind = '';
  const locator = { count: async () => 0, first() { return this; }, waitFor: async () => {} };
  const page = {
    url: () => currentUrl, locator: () => locator,
    async goto(url) {
      currentUrl = url;
      kind = url.includes('/members') ? 'members' : 'companies';
      if (mode === 'failed') throw new Error('Fixture navigation failed');
      if (mode === 'wrong-query') currentUrl = url.replace(encodeURIComponent(company), 'Ford');
      return { status: () => mode === '404' ? 404 : 200, url: () => currentUrl };
    },
    waitForLoadState: async () => {}, waitForTimeout: async () => {},
    async evaluate(fn, args) {
      if (dom) return fn(args);
      return { title: 'XING', text: '', links: kind === 'members' ? memberLinks : [] };
    },
  };
  const context = { URL, page };
  if (dom) {
    // The role/employer sit in a small card; the ancestor contains TWO profiles.
    // Capturing must stop at the identity boundary even without semantic tags.
    const link = { href: good.url, innerText: 'Anna Muster', textContent: 'Anna Muster' };
    const other = { href: foreign.url };
    const broad = { innerText: [...good.contextLines, ...foreign.contextLines].join('\n'), querySelectorAll: () => [link, other], matches: () => false, parentElement: null };
    const card = { innerText: good.contextLines.join('\n'), querySelectorAll: () => [link], matches: () => false, parentElement: broad };
    link.parentElement = card;
    context.document = { title: 'XING', body: { innerText: broad.innerText }, querySelectorAll: () => kind === 'members' ? [link] : [] };
  }
  const result = await vm.runInNewContext(`(async () => {${template}})()`, context, { timeout: 1000 });
  return JSON.parse(JSON.stringify(result));
}
const matched = await capture({ memberLinks: [good, foreign] });
assert.equal(matched.status, 'succeeded');
assert.equal(matched.records.length, 4);
assert.ok(matched.query_completion.every((item) => item.response_bound && item.query === company && item.result_completion === 'unconfirmed'));
const empty = await capture();
assert.equal(empty.status, 'no_extractable_fields');
assert.equal(empty.records.length, 0);
assert.ok(empty.query_completion.every((item) => item.response_bound));
for (const mode of ['failed', '404', 'wrong-query']) {
  const incomplete = await capture({ memberLinks: [good], mode });
  assert.equal(incomplete.status, 'capture_incomplete', mode);
  assert.equal(incomplete.records.length, 0, mode);
  assert.ok(incomplete.query_completion.every((item) => !item.response_bound), mode);
}
const isolated = await capture({ dom: true });
assert.equal(isolated.records.length, 4);
assert.ok(isolated.records.every((record) => record.note.includes(company) && !record.note.includes('Ford')));
console.log('XING production parser/capture: profile-bound employer quotes, no query-echo facts, foreign/former/same-name/neighbor exclusions, and response/query completion guards passed.');
