import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const fixture=mkdtempSync(join(tmpdir(),'outbound-import-app-'));
const source=fileURLToPath(new URL('../../customer-modules/outbound-lead-generation/',import.meta.url));
mkdirSync(join(fixture,'modules','olg'),{recursive:true});
mkdirSync(join(fixture,'shared'),{recursive:true});
writeFileSync(join(fixture,'package.json'),'{"type":"module"}');
for(const name of ['index.js','collection-reloader.mjs','lead-revision-loader.mjs','lead-list-loader.mjs','import-preview-groups.js','current-state-export.mjs', 'required-field-selection.mjs', 'read-error-grace.mjs', 'in-flight-lead-sweep.mjs'])
  copyFileSync(join(source,name),join(fixture,'modules','olg',name));
writeFileSync(join(fixture,'shared','universal-importer.js'), `
export async function extractCompanyRowsFromWorkbookFile(file,options) {
  if(options.withMeta!==true||options.includeSheets[0]!=='WZ-Code') throw new Error('missing workbook metadata options');
  return file.fixture;
}
export function extractCompanyRowsFromText(text) { return JSON.parse(text); }
export function normalizeCompanyRow(row,index) { return {...row,row_index:row.row_index??index}; }
export function openUniversalImporter() {}
export function parseDelimitedText(text) { return JSON.parse(text); }
`);
writeFileSync(join(fixture,'shared','dialogs.js'),
  'export async function showBusinessAlert() {}\nexport async function showBusinessConfirm(){return false;}\nexport async function showBusinessPrompt(){return null;}');
writeFileSync(join(fixture,'shared','i18n.js'),'export async function loadModuleMessages(){return {};}');

try {
  const hooks=(await import(pathToFileURL(join(fixture,'modules','olg','index.js')).href)).__leadgenOutboundTestHooks;
  const state=hooks.testState(); state.leads=[];
  let writes=0;
  state.collections={imports:{insert:async()=>{writes++;throw new Error('unexpected write');}},leads:{insert:async()=>{writes++;throw new Error('unexpected write');}}};
  const sheet=[['Kategorie1','Code','Listenname MUSTER'],['','20','Chemie'],['','64','Finanzdienstleistungen']];
  const row=(name,wz,i)=>({name,website:'',domain:'',country:'DE',city:'',row_index:i,raw:{'branche(wz)':wz}});
  const file=(rows)=>({source_type:'file',title:'UITEST-Import',source:{files:[{name:'Chemie.xlsx',fixture:{rows,meta:{skippedOutsideTable:16876,sheets:{'WZ-Code':sheet}}}}]}});
  const rows=[...Array.from({length:506},(_,i)=>row('Chemie '+i,'20 Chemie',i)),...Array.from({length:5001},(_,i)=>row('Finance '+i,'64 Finance',i+506))];
  await test('actual App preview selects chemistry, carries metadata and preserves group counts',async()=>{
    const first=await hooks.importPreview(file(rows));
    assert.equal(first.canProceed,false); assert.equal(first.groupLimit,5000);
    assert.equal(first.groups.reduce((n,g)=>n+g.count,0),5507);
    const group=first.groups.find(g=>g.label==='Chemie');
    const payload={...file(rows),selected_groups:[group.id]};
    const preview=await hooks.importPreview(payload);
    assert.equal(preview.canProceed,true); assert.equal(preview.groups.find(g=>g.id===group.id).selected,true);
    assert.ok(preview.items.some(i=>i.text.includes('16876')&&i.text.includes('außerhalb')));
    const analysis=await hooks.analyzeImportPayload(payload);
    assert.equal(analysis.validRows.length,506); assert.ok(analysis.validRows.every(r=>r.raw['branche(wz)']==='20 Chemie'));
  });
  await test('actual App rejects oversized and empty selections before any persistent write',async()=>{
    for(const ids of [[],['list:Finanzdienstleistungen']]){
      const payload={...file(rows),selected_groups:ids};
      const preview=await hooks.importPreview(payload); assert.equal(preview.canProceed,false);
      await assert.rejects(hooks.importPayload(payload),/5[.,]?000|5000/);
    }
    assert.equal(writes,0);
  });
  await test('actual App deduplicates after selection and hides unrelated row errors/hints',async()=>{
    const same=row('Same','64 Finance',0),chosen=row('Same','20 Chemie',1);
    const bad=row('','64 Finance',2); bad.website='mailto:bad@example.com';
    const a=await hooks.analyzeImportPayload({...file([same,chosen,bad]),selected_groups:['list:Chemie']});
    assert.equal(a.validRows.length,1); assert.equal(a.validRows[0].raw['branche(wz)'],'20 Chemie');
    assert.equal(a.invalidCount,0); assert.equal(a.hintCount,0); assert.equal(a.duplicateCount,0);
  });
  await test('prepared Sellify/resume rows retain imported evidence and never require workbook groups',async()=>{
    const original={...row('Sellify A','20 Chemie',0),contacts:[{person_key:'p1'}],evidence:[{source_id:'sellify'}],payload:{sellify_company_id:1}};
    const a=await hooks.analyzeImportPayload({source_type:'sellify',rows:[original,{...original}]});
    assert.equal(a.canProceed,true); assert.equal(a.groups,null); assert.equal(a.validRows.length,1);
    assert.deepEqual(a.validRows[0].contacts,original.contacts); assert.deepEqual(a.validRows[0].evidence,original.evidence);
    const legacy=await hooks.analyzeImportPayload({source_type:'file',source:{files:[{name:'old.xlsx',fixture:[row('Legacy','20 Chemie',0)]}]},selected_groups:['div:20']});
    assert.equal(legacy.canProceed,true); assert.equal(legacy.validRows.length,1);
  });
} finally { rmSync(fixture,{recursive:true,force:true}); }
