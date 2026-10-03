import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import vm from 'node:vm';
const baseline = process.argv.includes('--baseline');
const rust = baseline ? execFileSync('git', ['show',
  '5a8db7e28e448291c03e4cfb1fa1cc8ee10f75a2:src/core/service/business_os.rs'], {encoding:'utf8'})
  : readFileSync(new URL('./business_os.rs', import.meta.url), 'utf8');
const script = rust.match(/const ROCKETREACH_BROWSER_RECORD_PARSER: &str = r#"([\s\S]*?)"#;/)?.[1];
assert.ok(script, 'use the actual Rust-embedded parser, not a copied implementation');
const ctx=vm.createContext({URL});
vm.runInContext(script+'\nglobalThis.parse=parseRocketReachRecords;',ctx);
const parse=(company,snapshots)=>JSON.parse(JSON.stringify(ctx.parse(company,snapshots)));
const COMPANY='Example Manufacturing Europe AG';
const profile=(people=[],overrides={})=>({
 sourceUrl:'https://rocketreach.co/example-manufacturing-europe-profile_bcompany',
 title:COMPANY+' Company Profile | RocketReach',headings:[COMPANY],
 embeddedPeople:people,links:[],bodyLines:[COMPANY],...overrides
});
const person=(name,id,fields={})=>({
 name,company:COMPANY,sourceUrl:'https://rocketreach.co/'+name.replaceAll(' ','-')+'-email_'+id,
 ...fields
});
const values=(result,field)=>result.records.filter(r=>r.field===field).map(r=>r.value);
const cases=[];
const check=(name,fn)=>{try{fn();cases.push({name,ok:true});}catch(e){cases.push({name,ok:false,error:e.message});}};
check('current two-person contact fields carry exact quotes and distinct keys',()=>{
 const a=person('Ada Lovelace','bada',{email:'ada@example.test',phone:'+49 711 1234567',position:'Chief Technology Officer'});
 const b=person('Grace Hopper','bgrace',{email:'grace@example.test',position:'Managing Director'});
 const result=parse(COMPANY,[profile([a,b])]);
 assert.equal(result.companyMatched,true);
 assert.deepEqual(values(result,'person_email'),['ada@example.test','grace@example.test']);
 const keys=new Set(result.records.filter(r=>r.field.startsWith('person_')).map(r=>r.person_key));
 assert.equal(keys.size,2);assert.ok(!keys.has(undefined));
 for(const r of result.records){
  assert.ok(r.source_quote?.includes(r.value),r.field+' quote must name its actual value');
  assert.equal(r.note,r.source_quote);
  if(r.field.startsWith('person_'))assert.ok(r.source_quote.includes(COMPANY));
 }
 assert.equal(result.protectedFieldCount,result.records.filter(r=>r.field.startsWith('person_')).length);
});
check('all significant company words, not a prefix or substring, are required',()=>{
 assert.equal(parse(COMPANY,[profile([],{headings:['Example Manufacturing Asia AG'],title:'Example Manufacturing Asia AG'})]).companyMatched,false);
 const x=person('Ada Lovelace','bx',{company:'Example Manufacturing Asia AG'});
 assert.equal(parse(COMPANY,[profile([x])]).protectedFieldCount,0);
 const y=person('Ada Lovelace','by',{company:'Examples Manufacturing Europe AG'});
 assert.equal(parse(COMPANY,[profile([y])]).protectedFieldCount,0);
});
check('no requested company name is synthesized from a nameless link context',()=>{
 const result=parse(COMPANY,[{
  sourceUrl:'https://rocketreach.co/search',title:'Search',headings:[],
  links:[{url:'https://rocketreach.co/company/123',text:'',
   contextLines:[COMPANY]}]
 }]);
 assert.equal(result.companyMatched,false);assert.equal(result.records.length,0);
});
check('former employers, ended periods, and explicit different current employers are excluded',()=>{
 for(const context of ['Former '+COMPANY,COMPANY+' 2005–2020',COMPANY+' bis 2020']){
  const x=person('Ada Lovelace','bold',{company:undefined,contextLines:['Ada Lovelace',context,'ada@example.test']});
  assert.equal(parse(COMPANY,[profile([x])]).protectedFieldCount,0,context);
 }
 const x=person('Ada Lovelace','bwrong',{company:'Other Industries AG',
  contextLines:['Ada Lovelace',COMPANY,'ada@example.test']});
 assert.equal(parse(COMPANY,[profile([x])]).protectedFieldCount,0);
});
check('academic prefix is observed; a job title and a name never become academic title or gender',()=>{
 const result=parse(COMPANY,[profile([
  person('Prof. Dr. Ada Lovelace','bada',{position:'Managing Director'}),
  person('Grace Hopper','bgrace',{position:'Professor of Chemistry'})
 ])]);
 assert.deepEqual(values(result,'person_titel'),['Prof. Dr.']);
 assert.equal(values(result,'person_geschlecht').length,0);
 assert.ok(result.records.find(r=>r.field==='person_titel').source_quote.includes('Prof. Dr. Ada Lovelace'));
});
check('additional capture for the same provider person fills missing fields',()=>{
 const first=person('Ada Lovelace','bada',{sourceUrl:'https://rocketreach.com/ada-lovelace-email_bada'});
 const second=person('Ada Lovelace','bada',{sourceUrl:'https://rocketreach.com/ada-lovelace-email_bada',email:'ada@example.test'});
 const result=parse(COMPANY,[profile([first,second])]);
 assert.deepEqual(values(result,'person_email'),['ada@example.test']);
 assert.equal(new Set(result.records.filter(r=>r.field.startsWith('person_')).map(r=>r.person_key)).size,1);
});
check('different names on one provider identity are never merged',()=>{
 const result=parse(COMPANY,[profile([
  person('Ada Lovelace','bsame',{email:'ada@example.test'}),
  person('Grace Hopper','bsame',{email:'grace@example.test'})
 ])]);
 assert.equal(result.protectedFieldCount,0);
});
check('malformed snapshots and scalar fields do not discard another valid person',()=>{
 const a=person('Ada Lovelace','bada',{email:{value:'invented@example.test'},position:['CEO']});
 const b=person('Grace Hopper','bgrace',{email:'grace@example.test'});
 const result=parse(COMPANY,[null,{},profile([a,b],{links:{},bodyLines:false}),profile([],{headings:{},embeddedPeople:17})]);
 assert.deepEqual(values(result,'person_email'),['grace@example.test']);
 assert.ok(!result.records.some(r=>String(r.value).includes('[object Object]')));
 assert.deepEqual(values(result,'person_vorname'),['Ada','Grace']);
});
check('only actual HTTPS provider profile origins are accepted',()=>{
 for(const url of ['http://rocketreach.co/example-profile_bcompany',
  'https://rocketreach.co.evil.test/example-profile_bcompany',
  'https://user:pass@rocketreach.co/example-profile_bcompany',
  'https://rocketreach.co:8443/example-profile_bcompany']){
  assert.equal(parse(COMPANY,[profile([],{sourceUrl:url})]).records.length,0,url);
 }
 assert.equal(parse(COMPANY,[profile([person('Ada Lovelace','bada')],{title:'Log in | RocketReach'})]).records.length,0);
});
check('tracking parameters and fragments never leak into evidence URLs',()=>{
 const result=parse(COMPANY,[profile([person('Ada Lovelace','bada',{
  sourceUrl:'https://rocketreach.co/ada-lovelace-email_bada?tracking=opaque#contact',
  email:'ada@example.test'})])]);
 assert.ok(result.records.every(r=>!r.source_url.includes('?')&&!r.source_url.includes('#')));
});
check('existing scoped-link capture still returns company and five protected contact fields',()=>{
 const result=parse('Example Manufacturing AG',[{
  sourceUrl:'https://rocketreach.co/example-manufacturing-ag-profile_bexample',
  title:'Example Manufacturing AG Company Profile | RocketReach',headings:['Example Manufacturing AG'],
  bodyLines:['Example Manufacturing AG'],embeddedPeople:[],
  links:[{url:'https://rocketreach.com/ada-lovelace-email_bexample',text:'Ada Lovelace',
   contextLines:['Ada Lovelace','Chief Technology Officer','Example Manufacturing AG',
    'ada.lovelace@example.test','+49 711 1234567']}]
 }]);
 assert.equal(result.companyMatched,true);
 for(const field of ['firma_name','person_vorname','person_nachname','person_position','person_email','person_telefon'])
  assert.ok(values(result,field).length>0,field);
});
check('whole profile body cannot attribute footer or another person contact to the heading owner',()=>{
 const own=person('Ada Lovelace','bada').sourceUrl;
 const result=parse(COMPANY,[profile(),{
  sourceUrl:own,title:'Ada Lovelace | RocketReach',headings:['Ada Lovelace'],links:[],embeddedPeople:[],
  bodyLines:['Ada Lovelace','Managing Director',COMPANY,'Contact Grace Hopper: grace@example.test','Support: support@rocketreach.com','+49 711 9999999']
 }]);
 assert.deepEqual(values(result,'person_vorname'),['Ada']);
 assert.equal(values(result,'person_email').length,0);
 assert.equal(values(result,'person_telefon').length,0);
});
check('structured owner contacts remain available when the same profile has unrelated body contacts',()=>{
 const own=person('Ada Lovelace','bada',{email:'ada@example.test',phone:'+49 711 1234567'});
 const result=parse(COMPANY,[profile(),profile([own],{sourceUrl:own.sourceUrl,headings:['Ada Lovelace'],title:'Ada Lovelace | RocketReach',bodyLines:[COMPANY,'support@rocketreach.com','+49 711 9999999']})]);
 assert.deepEqual(values(result,'person_email'),['ada@example.test']);
 assert.deepEqual(values(result,'person_telefon'),['+49 711 1234567']);
});
check('whole profile body navigation cannot become the person job position',()=>{
 const own=person('Ada Lovelace','bada').sourceUrl;
 const result=parse(COMPANY,[profile(),{
  sourceUrl:own,title:'Ada Lovelace | RocketReach',headings:['Ada Lovelace'],links:[],embeddedPeople:[],
  bodyLines:['Find colleagues','Pricing and support','Ada Lovelace',COMPANY]
 }]);
 assert.deepEqual(values(result,'person_vorname'),['Ada']);
 assert.equal(values(result,'person_position').length,0);
});

const adapterArg = process.argv.indexOf('--adapter');
const adapterSource = adapterArg >= 0 ? readFileSync(process.argv[adapterArg+1], 'utf8')
 : readFileSync(new URL('../../tools/web-stack/scrape-targets/rocketreach.com/rocketreach-native-capture.js', import.meta.url), 'utf8');
const adapterHead = adapterSource.split('\n(function main() {')[0];
const pipelineRecords = () => parse(COMPANY,[profile([
 person('Ada Lovelace','bada',{email:'ada@example.test',position:'Chief Technology Officer'}),
 person('Grace Hopper','bgrace',{email:'grace@example.test'})
])]).records;
function adapterRun(input, responder, {main=false, clockStep=0}={}) {
 let elapsed=0, output='';
 const calls=[];
 class Clock extends Date { static now() {return elapsed;} }
 const scope=vm.createContext({URL,Date:Clock,console:{warn(){}},
  process:{env:{CTOX_SCRAPE_INPUT_JSON:JSON.stringify(input && !Array.isArray(input) && typeof input === 'object'
    ? {source_id:'rocketreach.com',...input} : input)},pid:123,cwd:()=>'/fixture',
   stdout:{write(value){output+=value;}}},
  require(name){
   if(name==='child_process')return {execFileSync(binary,args,options){
    calls.push({binary,args,options});
    elapsed+=clockStep;
    const result=responder(args,calls.length);
    return JSON.stringify(result);
   }};
   if(name==='fs')return {writeFileSync(){throw Error('unexpected public browser fallback');},unlinkSync(){}};
   if(name==='path')return {join:(...segments)=>segments.join('/')};
   throw Error('unexpected dependency '+name);
  }
 });
 vm.runInContext((main?adapterSource:adapterHead+'\nglobalThis.api={acceptedProviderRecords,runCtox};'),scope,{timeout:1000});
 return {scope,calls,result:output?JSON.parse(output):null};
}
const accepted=(records)=>JSON.parse(JSON.stringify(adapterRun({},()=>{throw Error('no provider calls expected');})
 .scope.api.acceptedProviderRecords('rocketreach.com',COMPANY,records)));
check('native-to-runtime pipeline preserves two separately quoted persons',()=>{
 const records=accepted(pipelineRecords());
 assert.deepEqual(records.filter(r=>r.field==='person_email').map(r=>r.value),['ada@example.test','grace@example.test']);
 assert.equal(new Set(records.filter(r=>r.field.startsWith('person_')).map(r=>r.person_key)).size,2);
 for(const record of records){
  assert.ok(record.source_quote.includes(record.value));
  assert.equal(record.source_id,'rocketreach.com');assert.ok(record.observed_at);
 }
});
check('runtime rejects missing literal quotes, malformed values and foreign keys',()=>{
 const records=pipelineRecords();
 const a=records.find(r=>r.field==='person_email');
 for(const bad of [
  {...a,source_quote:undefined,note:'Provider contact'},
  {...a,value:{value:'invented@example.test'}},
  {...a,person_key:'rocketreach-person-other'},
  {...a,source_url:'https://foreign.test/ada-email_bada'},
  {...a,source_url:'https://user:secret@rocketreach.co/ada-email_bada'},
  {...a,source_url:'http://rocketreach.co/ada-email_bada'}
 ]){
  const result=accepted([...records.filter(r=>r.field!=='person_email'),bad]);
  assert.equal(result.filter(r=>r.field==='person_email').length,0);
 }
});
check('runtime rejects company-prefix and conflicting person identity records',()=>{
 const records=pipelineRecords();
 assert.equal(accepted(records.map(r=>({...r,
  value:r.field==='firma_name'?'Example Manufacturing Asia AG':r.value,
  note:r.note.replaceAll('Europe','Asia'),source_quote:r.source_quote.replaceAll('Europe','Asia')
 }))).length,0);
 const ada=records.find(r=>r.field==='person_vorname'&&r.value==='Ada');
 const conflicting={...ada,value:'Other',note:'Other Lovelace '+COMPANY,source_quote:'Other Lovelace '+COMPANY};
 const result=accepted([...records,conflicting]);
 assert.ok(!result.some(r=>r.person_key===ada.person_key));
 assert.ok(result.some(r=>r.value==='grace@example.test'));
});
check('runtime never attributes another person quote to the keyed contact',()=>{
 const records=pipelineRecords();
 const ada=records.find(record=>record.field==='person_email'&&record.value==='ada@example.test');
 for(const wrongOwner of ['Grace Hopper','Ada Lovelacee','NotAda Lovelace']){
  const quote=wrongOwner+' · '+COMPANY+' · '+ada.value;
  const result=accepted([...records.filter(record=>record!==ada),{...ada,note:quote,source_quote:quote}]);
  assert.ok(!result.some(record=>record.field==='person_email'&&record.person_key===ada.person_key),wrongOwner);
  assert.ok(result.some(record=>record.value==='grace@example.test'));
  assert.ok(result.some(record=>record.field==='person_vorname'&&record.value==='Ada'));
 }
});
check('runtime invalid/null input completes explicitly without any CLI call',()=>{
 for(const input of [null,[],{}, {company:{}},{company:COMPANY,country:{}},
  {company:COMPANY,source_id:'xing.com'}]){
  const run=adapterRun(input,()=>{throw Error('invalid input reached provider');},{main:true});
  assert.equal(run.calls.length,0);assert.equal(run.result.failure_mode,'invalid_input');
 }
});
check('runtime successful native capture keeps evidence and owner task binding',()=>{
 const run=adapterRun({company:COMPANY,task_id:'queue:system::fixture'},args=>{
  assert.equal(args[2],'source-capture');
  assert.ok(args.includes('queue:system::fixture'));
  return {ok:true,source_status:'succeeded',records:pipelineRecords()};
 },{main:true});
 assert.equal(run.calls.length,1);
 assert.equal(run.result.records.filter(r=>r.field==='person_email').length,2);
 assert.equal(run.result.failure_mode,undefined);
 assert.ok(run.calls[0].options.timeout>0&&run.calls[0].options.timeout<=65000);
 assert.ok(run.calls[0].options.maxBuffer<=4*1024*1024);
});
check('missing access is authorization_required and never falls back to public guessing',()=>{
 const run=adapterRun({company:COMPANY},args=>{
  if(args[2]==='source-capture')return {ok:false,source_status:'authorization_required',records:[]};
  if(args[2]==='auth-assist-login')return {ok:false,reason:'credential_missing'};
  if(args[2]==='auth-assist-request')return {ok:true,target_url:'https://rocketreach.co/login',
   allowed_domains:['rocketreach.co'],session_id:'fixture'};
  throw Error('unexpected fallback '+args.join(' '));
 },{main:true});
 assert.equal(run.result.failure_mode,'authorization_required');
 assert.equal(run.result.records.length,0);assert.equal(run.result.browser_assist_requested,true);
 assert.equal(run.calls.length,3);
});
check('login completion recaptures once with returned session and owner task',()=>{
 let captures=0;
 const run=adapterRun({company:COMPANY,task_id:'queue:system::fixture'},args=>{
  if(args[2]==='auth-assist-login')return {ok:true,target_url:'https://rocketreach.co/login',
   session_id:'session-fixture',allowed_domains:['rocketreach.co']};
  if(args[2]==='source-capture'){
   captures++;
   if(captures===1)return {ok:false,source_status:'auth_required',records:[]};
   assert.ok(args.includes('session-fixture'));assert.ok(args.includes('queue:system::fixture'));
   return {ok:true,source_status:'succeeded',records:pipelineRecords()};
  }
  throw Error('unexpected request');
 },{main:true});
 assert.equal(run.calls.length,3);assert.ok(run.result.records.length>0);
});
check('runtime rejects unsafe provider login and browser-assist targets',()=>{
 for(const target of ['http://rocketreach.co/login','https://rocketreach.co:8443/login',
  'https://user:secret@rocketreach.co/login','https://rocketreach.co.attacker.test/login']){
  const run=adapterRun({company:COMPANY},args=>{
   if(args[2]==='source-capture')return {ok:false,source_status:'authorization_required',records:[]};
   if(args[2]==='auth-assist-login'||args[2]==='auth-assist-request')
    return {ok:true,target_url:target,session_id:'session-fixture',allowed_domains:['rocketreach.co']};
   throw Error('unexpected request');
  },{main:true});
  assert.equal(run.calls.filter(call=>call.args[2]==='source-capture').length,1,target);
  assert.equal(run.result.failure_mode,'authorization_required');
  assert.equal(run.result.browser_assist_requested,false,target);
 }
});
check('native timeout ends explicitly and never starts a login or another task',()=>{
 const run=adapterRun({company:COMPANY},()=>{const e=new Error('timeout');e.code='ETIMEDOUT';throw e;},{main:true});
 assert.equal(run.calls.length,1);assert.equal(run.result.failure_mode,'temporary_unreachable');
 assert.match(run.result.detail,/timed out/);assert.equal(run.result.browser_assist_requested,false);
});
check('shared adapter deadline suppresses later subprocesses after budget is exhausted',()=>{
 const run=adapterRun({company:COMPANY},args=>args[2]==='source-capture'
  ?{ok:false,source_status:'authorization_required',records:[]}:{ok:false,reason:'credential_missing'},
  {main:true,clockStep:100000});
 assert.equal(run.calls.length,2);
 assert.equal(run.result.failure_mode,'authorization_required');
 assert.equal(run.result.browser_assist_requested,false);
});
check('successful malformed native capture is not a completed negative query',()=>{
 const run=adapterRun({company:COMPANY},()=>({ok:true,source_status:'succeeded',
  records:[{field:'firma_name',value:COMPANY,note:'generic provider note',
   source_url:'https://rocketreach.co/example-profile_bcompany'}]}),{main:true});
 assert.equal(run.calls.length,1);assert.equal(run.result.records.length,0);
 assert.equal(run.result.failure_mode,'temporary_unreachable');
});

// Execute the actual browser template too: parser-only tests cannot catch a
// lost snapshot binding or cumulative navigation beyond the native deadline.
const captureTemplate=rust.match(/const ROCKETREACH_BROWSER_CAPTURE_TEMPLATE: &str = r#"([\s\S]*?)"#;/)?.[1];
assert.ok(captureTemplate);
async function captureRun({url='https://rocketreach.co/search',links=[],people=[],navigationMs=0}={}) {
 let elapsed=0;
 const calls=[];
 const title=COMPANY+' Company Profile | RocketReach';
 const anchors=links.map(link=>({href:link.url,innerText:link.text,textContent:link.text,
  closest:()=>({innerText:(link.contextLines||[]).join('\n'),
   querySelectorAll:()=> (link.contextProfiles||[link.url]).map(href=>({href}))})}));
 const document={title,body:{innerText:COMPANY},querySelectorAll(selector){
  if(selector==='a[href]')return anchors;
  if(selector.startsWith('h1'))return [{innerText:COMPANY}];
  if(selector.startsWith('script'))return [{textContent:JSON.stringify({people})}];
  return [];
 }};
 const scope=vm.createContext({URL,Date:class extends Date {static now(){return elapsed;}},
  document,location:{get href(){return url;}},
  page:{url:()=>url,evaluate:async fn=>vm.runInContext('('+fn.toString()+')()',scope),
   waitForLoadState:async (_state,options)=>{calls.push({kind:'wait',...options});},
   locator:()=>({first(){return this;},count:async()=>0,
    waitFor:async options=>{calls.push({kind:'wait',...options});}})},
  ctoxBrowser:{goto:async (target,options)=>{
   calls.push({kind:'goto',target,at:elapsed,...options});
   elapsed+=Math.min(navigationMs,options.timeoutMs);url=target;
  }}
 });
 const source=captureTemplate.replace('__COMPANY_JSON__',JSON.stringify(COMPANY))
  .replace('__COUNTRY_JSON__','"DE"').replace('__RECORD_PARSER__',script);
 try{return {result:JSON.parse(JSON.stringify(await vm.runInContext('(async()=>{'+source+'})()',scope))),calls,elapsed};}
 catch(error){return {error,calls,elapsed};}
}
async function captureCheck(name,fn){try{await fn();cases.push({name,ok:true});}
 catch(error){cases.push({name,ok:false,error:error.message});}}
await captureCheck('browser capture retains the snapshot that owns a company link',async()=>{
 const link={url:'https://rocketreach.co/example-manufacturing-europe-profile_bcompany',text:COMPANY,contextLines:[COMPANY]};
 const run=await captureRun({links:[link]});
 assert.equal(run.error,undefined,run.error?.message);
 assert.ok(run.calls.some(call=>call.kind==='goto'&&call.target===link.url));
});
await captureCheck('browser capture does not navigate credential-bearing or HTTP provider links',async()=>{
 for(const url of ['http://rocketreach.co/example-profile_bcompany',
  'https://user:secret@rocketreach.co/example-profile_bcompany',
  'https://rocketreach.co:8443/example-profile_bcompany']){
  const run=await captureRun({links:[{url,text:COMPANY,contextLines:[COMPANY]}]});
  assert.equal(run.error,undefined,run.error?.message);
  assert.ok(!run.calls.some(call=>call.kind==='goto'&&call.target===url),url);
 }
});
await captureCheck('browser capture never scopes a shared results container to one person',async()=>{
 const url='https://rocketreach.co/example-manufacturing-europe-profile_bcompany';
 const ada='https://rocketreach.co/ada-lovelace-email_bada';
 const grace='https://rocketreach.co/grace-hopper-email_bgrace';
 const mixed=await captureRun({url,links:[{url:ada,text:'Ada Lovelace',
  contextProfiles:[ada,grace],contextLines:['Ada Lovelace',COMPANY,'Grace Hopper','grace@example.test','+49 711 7654321']}]});
 assert.equal(mixed.error,undefined,mixed.error?.message);
 assert.ok(!mixed.result.records.some(record=>record.person_key==='rocketreach-person-bada'
  && ['person_email','person_telefon','person_position'].includes(record.field)));
 const own=await captureRun({url,links:[{url:ada,text:'Ada Lovelace',
  contextProfiles:[ada,ada],contextLines:['Ada Lovelace',COMPANY,'ada@example.test','+49 711 1234567']}]});
 assert.equal(own.error,undefined,own.error?.message);
 assert.ok(own.result.records.some(record=>record.person_key==='rocketreach-person-bada'
  &&record.field==='person_email'&&record.value==='ada@example.test'));
});
await captureCheck('browser capture preserves explicitly separated compound given and family names',async()=>{
 const run=await captureRun({url:'https://rocketreach.co/example-manufacturing-europe-profile_bcompany',people:[{
  first_name:'Mary Ann',last_name:'van der Berg',company_name:COMPANY,
  profile_url:'https://rocketreach.co/mary-ann-van-der-berg-email_bmary',work_email:'mary@example.test'
 }]});
 assert.equal(run.error,undefined,run.error?.message);
 assert.deepEqual(values(run.result,'person_vorname'),['Mary Ann']);
 assert.deepEqual(values(run.result,'person_nachname'),['van der Berg']);
});
await captureCheck('browser capture honours explicit current employer over a historical company object',async()=>{
 const run=await captureRun({url:'https://rocketreach.co/example-manufacturing-europe-profile_bcompany',people:[{
  firstName:'Ada',lastName:'Lovelace',company:{name:COMPANY},currentCompany:'Other Industries AG',
  profileUrl:'https://rocketreach.co/ada-lovelace-email_bada',email:'ada@example.test'
 }]});
 assert.equal(run.error,undefined,run.error?.message);
 assert.equal(run.result.records.filter(record=>record.field.startsWith('person_')).length,0);
});
await captureCheck('browser navigation shares one deadline instead of five independent 30-second budgets',async()=>{
 const people=Array.from({length:5},(_,i)=>({url:'https://rocketreach.co/ada-lovelace-email_b'+i,text:'Ada Lovelace',contextLines:['Ada Lovelace',COMPANY]}));
 const run=await captureRun({url:'https://rocketreach.co/example-manufacturing-europe-profile_bcompany',links:people,navigationMs:30000});
 assert.ok(run.elapsed<=55000,'capture consumed '+run.elapsed+'ms');
 assert.ok(run.error||run.result.status!=='no_match','exhaustion is not a completed negative query');
 for(const call of run.calls.filter(call=>call.kind==='goto'))assert.ok(call.timeoutMs<=55000-call.at);
});

const failed=cases.filter(c=>!c.ok);
const report={baseline,read_at_utc:new Date().toISOString(),
 native_parser_sha256:createHash('sha256').update(script).digest('hex'),
 runtime_adapter_sha256:createHash('sha256').update(adapterSource).digest('hex'),
 scope:'Actual embedded native parser and runtime adapter, with mock CLI and clocks; source fixtures, not compiled or installed native/browser/lead acceptance',
 passed:cases.length-failed.length,failed:failed.length,cases};
console.log(JSON.stringify(report,null,2));
if(process.env.ROCKETREACH_TEST_RESULT)writeFileSync(process.env.ROCKETREACH_TEST_RESULT,JSON.stringify(report,null,2)+'\n',{mode:0o600});
process.exitCode=failed.length?1:0;
