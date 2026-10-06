import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const file = new URL('../../customer-modules/outbound-lead-generation/import-preview-groups.js', import.meta.url);
const api = await import('data:text/javascript;base64,' + Buffer.from(readFileSync(file, 'utf8')).toString('base64'));
const { normalizeWzDivision, buildWzMapping, selectImportGroups } = api;
let passed = 0;
function test(name, run) { run(); console.log('PASS ' + name); passed++; }
const sheet = Object.freeze([
  Object.freeze(['Kategorie1', '\uFEFF Code ', ' LISTENNAME THESEN ']),
  Object.freeze(['', '20', 'Herstellung von chemischen Erzeugnissen']),
  Object.freeze(['', '21', 'Herstellung von chemischen Erzeugnissen']),
  Object.freeze(['', '64', 'Erbringung von Finanzdienstleistungen']),
  Object.freeze(['', '46', 'Großhandel']),
  Object.freeze(['', '28', 'Maschinenbau']),
  Object.freeze(['', '99', 'Sonstige']),
  Object.freeze(['', '03', 'Fischerei']),
]);
const company = (id, wz) => Object.freeze({ id, name: 'Firma ' + id, raw: Object.freeze({ 'branche(wz)': wz }) });
test('WZ division normalization distinguishes absent codes and strips leading zero', () => {
  for (const [input, wanted] of [['64.21 Beteiligungsgesellschaften','64'], ['01 Landwirtschaft','1'], [20,'20'], [' 03.1 ','3'], [null,''], ['', ''], ['Chemie20',''], [3,'']]) assert.equal(normalizeWzDivision(input), wanted);
});
test('Mapping recognizes reordered/BOM/case headers, numeric codes and invalid rows', () => {
  const data = [[' listenname thesen ', '\uFEFF CODE'], [' A ',3], ['B','20'], ['invalid','x20'], ['invalid',2.5], ['','04'], []];
  const before = JSON.stringify(data);
  assert.deepEqual([...buildWzMapping(data)], [['3','A'],['20','B']]);
  assert.equal(JSON.stringify(data),before);
  for (const invalid of [null,[],[[]],[['Other','Code'],['A','20']]]) assert.equal(buildWzMapping(invalid).size,0);
});
const fixtures = [];
for (const [n,wz] of [[17000,''],[6860,'64.21 Beteiligungsgesellschaften'],[3704,'46 Großhandel'],[1374,'28 Maschinenbau'],[506,'20 Chemie'],[3482,'99 Sonstige']]) {
  for (let i=0;i<n;i++) fixtures.push(company(fixtures.length,wz));
}
test('32926-row catalog counts all companies and selects exactly the506 chemistry rows', () => {
  const first = selectImportGroups(fixtures,sheet);
  assert.equal(fixtures.length,32926);
  assert.equal(first.groups.reduce((n,g)=>n+g.count,0),32926);
  assert.deepEqual(first.groups.map(g=>g.count),[17000,6860,3704,3482,1374,506]);
  assert.equal(first.selectedCount,0); assert.equal(first.canProceed,false);
  const chem=first.groups.find(g=>g.label==='Herstellung von chemischen Erzeugnissen');
  const chosen=selectImportGroups(fixtures,sheet,[chem.id]);
  assert.equal(chosen.selectedCount,506); assert.equal(chosen.canProceed,true);
  assert.ok(chosen.selectedRows.every(r=>fixtures.includes(r)&&r.raw['branche(wz)']==='20 Chemie'));
});
test('5000 limit rejects oversized selections without silent truncation', () => {
  const finance=selectImportGroups(fixtures,sheet).groups.find(g=>g.label==='Erbringung von Finanzdienstleistungen');
  const big=selectImportGroups(fixtures,sheet,[finance.id]);
  assert.equal(big.selectedCount,6860); assert.equal(big.selectedRows.length,6860); assert.equal(big.canProceed,false); assert.match(big.message,/5000/);
  const allChem=Array.from({length:5001},(_,i)=>company(i,'20 Chemie'));
  const id=selectImportGroups(allChem,sheet).groups[0].id;
  assert.equal(selectImportGroups(allChem.slice(0,5000),sheet,[id]).canProceed,true);
  assert.equal(selectImportGroups(allChem,sheet,[id]).canProceed,false);
});
test('Empty/unknown selections cannot import rows; mapped lists merge across divisions', () => {
  const rows=[company(1,'20 Chemie'),company(2,'21 Pharma'),company(3,'')];
  const first=selectImportGroups(rows,sheet);
  assert.equal(first.groups.find(g=>g.label==='Herstellung von chemischen Erzeugnissen').count,2);
  for(const selected of [undefined,[],['unknown'],[null,42]]) {
    const r=selectImportGroups(rows,sheet,selected); assert.equal(r.selectedCount,0); assert.equal(r.canProceed,false);
  }
  const id=first.groups.find(g=>g.count===2).id;
  assert.deepEqual(selectImportGroups(rows,sheet,[id,id]).selectedRows,rows.slice(0,2));
});
test('Absent mapping and maliciously similar labels remain distinct stable groups', () => {
  const rows=[company(1,'20 Chemie'),company(2,'')];
  const fallback=selectImportGroups(rows,null);
  assert.ok(fallback.groups.some(g=>g.label==='WZ-Abteilung 20'));
  const mapped=selectImportGroups(rows,[['Code','Listenname THESEN'],['20','(ohne WZ-Code)']]);
  assert.equal(mapped.groups.length,2); assert.equal(new Set(mapped.groups.map(g=>g.id)).size,2);
  const one=mapped.groups.find(g=>g.id.startsWith('list:')).id;
  assert.deepEqual(selectImportGroups(rows,[['Code','Listenname THESEN'],['20','(ohne WZ-Code)']],[one]).selectedRows,[rows[0]]);
  assert.deepEqual(selectImportGroups([...rows].reverse(),sheet).groups.map(g=>g.id).sort(),selectImportGroups(rows,sheet).groups.map(g=>g.id).sort());
});
console.log(passed + ' import grouping regressions passed');
