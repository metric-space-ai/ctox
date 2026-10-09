import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { workspaceDataState } from '../modules/ctox/data-state.js';
const source = readFileSync(new URL('../modules/ctox/index.js', import.meta.url), 'utf8');
const shell = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
function body(name, text = source) {
  const from = text.indexOf(`function ${name}(`); assert(from >= 0, name);
  const start = text.slice(Math.max(0, from - 6), from) === 'async ' ? from - 6 : from;
  return text.slice(start, text.indexOf('\n}', from) + 2);
}
const names = ['ctoxCollection','isInternalSmokeDoc','isTaskSourceReadDenied','loadLocalCollection','loadLocalCommands','loadLocalQueueTasks','loadTaskSource'];
const reads = Function('LOCAL_COLLECTION_LIMIT', names.map(n => body(n)).join('\n')+'\nreturn {'+names.join(',')+'};')(200);
const denied = name => ({code:'UNAUTHORIZED',message:`peer is not authorized for collection ${name}`});
const sleep = () => new Promise(resolve => setTimeout(resolve, 30));
function database(exec) {
  return {collection:name => ({find(query) { return {limit() { return this; },exec:() => exec(name,query)}; }})};
}

test('native denied task source gets one bounded read, retires exact listeners and never sorts around authority', async () => {
  const queries=[], stops=[]; const refusal=denied('ctox_queue_tasks');
  const state={ctx:{db:database((name,q) => { queries.push({name,q}); return Promise.reject(refusal); })},
    localCollectionCleanups:new Map([['ctox_queue_tasks',() => stops.push('local')]]),
    readinessCollectionCleanups:new Map([['ctox_queue_tasks',() => stops.push('readiness')]])};
  for (let i=0;i<20;i++) assert.deepEqual(await reads.loadTaskSource(state,'ctox_queue_tasks',reads.loadLocalQueueTasks),[]);
  assert.equal(queries.length,1); assert.equal(queries[0].q.limit,200);
  assert.equal(state.taskSourceUnavailable.get('ctox_queue_tasks'),refusal); assert.deepEqual(stops,['local','readiness']);
  const fresh={ctx:{db:database(async () => [{toJSON:() => ({id:'new-authorized-task'})}])}};
  assert.equal((await reads.loadTaskSource(fresh,'ctox_queue_tasks',reads.loadLocalQueueTasks))[0].id,'new-authorized-task');
  assert.equal(fresh.taskSourceUnavailable,undefined);
});

test('permission classification is scoped; expired sessions, wrong collections and uncoded denials remain errors', async () => {
  for(const error of [new Error('denied'),{code:'UNAUTHORIZED',message:'session expired'},denied('other_collection'),
    {code:'CTOX_BUSINESS_OS_PERMISSION_DENIED',details:{collection:'ctox_queue_tasks',permission:'data.write'}}]) {
    assert.equal(reads.isTaskSourceReadDenied(error,'ctox_queue_tasks'),false);
    const state={ctx:{}}; await assert.rejects(reads.loadTaskSource(state,'ctox_queue_tasks',async()=>{throw error;}),e=>e===error);
    assert.equal(state.taskSourceUnavailable,undefined);
  }
  assert.equal(reads.isTaskSourceReadDenied('UNAUTHORIZED: peer is not authorized for collection ctox_queue_tasks','ctox_queue_tasks'),true);
  assert.equal(reads.isTaskSourceReadDenied({code:'COLLECTION_READ_FORBIDDEN'},'ctox_queue_tasks'),true);
});

test('supported sort fallback still works and authorized commands stay available beside a denied queue', async () => {
  const queries=[];
  const ctx={db:database(async (name,q) => {
    queries.push({name,q}); if(name==='ctox_queue_tasks') throw denied(name);
    if(q?.sort) throw new Error('unsupported sort'); return [{toJSON:() => ({id:'own-command',updated_at_ms:1})}];
  })};
  const state={ctx}; await reads.loadTaskSource(state,'ctox_queue_tasks',reads.loadLocalQueueTasks);
  assert.equal((await reads.loadTaskSource(state,'business_commands',reads.loadLocalCommands))[0].id,'own-command');
  assert.equal(queries.filter(q=>q.name==='ctox_queue_tasks').length,1);
  assert.equal(queries.filter(q=>q.name==='business_commands').length,2);
});

function realtime(state, render, errors=[]) {
  return Function('ctoxCollection','changeConcernsSelectedTask','refreshConfirmedHarnessStatus','renderFromLocalCache',
    'subscribeTaskHistoryChanges','showDataError','window','LOCAL_RENDER_DEBOUNCE_MS','console',body('wireLocalRealtime')+'\nreturn wireLocalRealtime;')(
      reads.ctoxCollection,()=>true,()=>{},render,()=>()=>{},(_s,e)=>errors.push(e),{setTimeout,clearTimeout},1,
      {warn:(_message,e)=>errors.push(e)})(state);
}

test('actual realtime uses invalidations instead of denied eager collection snapshots and cleans up once', async () => {
  const subscriptions=new Map(), stopped=[]; let paints=0;
  const state={ctx:{db:{collection:name => ({$:{subscribe(next,options) {
    assert.equal(options.invalidateOnly,name!=='ctox_harness_status');
    subscriptions.set(name,{next,options}); return {unsubscribe:()=>stopped.push(name)};
  }}})}}};
  const stop=realtime(state,async()=>{paints++;}); subscriptions.get('ctox_queue_tasks').next({}); await sleep();
  assert.equal(paints,1); stop();stop();
  assert.equal(stopped.length,new Set(stopped).size); assert.equal(state.localCollectionCleanups.size,0);
});

test('actual readiness callbacks retire a denied source and do not repeatedly hydrate it', async () => {
  let queueReads=0,hydrates=0; const callbacks=new Map(),unsubscribes=[];
  const state={lang:'en',ctx:{db:database(async name=>{if(name==='ctox_queue_tasks'){queueReads++;throw denied(name);}return [];}),
    sync:{subscribeCollectionReadiness(name,next){callbacks.set(name,next);next({ready:false});return()=>unsubscribes.push(name);}}}};
  const render=Function('hydrateFromLocal','showDataError','window','LOCAL_RENDER_DEBOUNCE_MS','console',body('renderFromLocalCache')+'\nreturn renderFromLocalCache;')(
    async s=>{hydrates++;await reads.loadTaskSource(s,'ctox_queue_tasks',reads.loadLocalQueueTasks);},()=>assert.fail('unexpected hydration error'),{setTimeout},1,{warn:()=>assert.fail('unexpected warning')});
  const wire=Function('TASK_SOURCE_COLLECTIONS','wireLocalRealtime','renderFromLocalCache','console',body('wireTaskSourceReadiness')+'\nreturn wireTaskSourceReadiness;')(
    ['ctox_queue_tasks','business_commands','ctox_bug_reports'],()=>()=>{},render,{warn:()=>assert.fail('unexpected readiness warning')});
  const stop=wire(state);await sleep();const before=hydrates;
  for(let i=0;i<10;i++)callbacks.get('ctox_queue_tasks')({ready:true});await sleep();
  assert.equal(queueReads,1);assert.equal(hydrates,before);assert.equal(unsubscribes.filter(n=>n==='ctox_queue_tasks').length,1);
  callbacks.get('business_commands')({ready:true});await sleep();assert.equal(queueReads,1);assert(hydrates>before);stop();stop();
  assert.equal(unsubscribes.length,new Set(unsubscribes).size);
});

test('restricted task data is visible as restricted, never idle, syncing or retryable', () => {
  const labels={de:{tasks:'Aufgaben',notPermittedForRole:'für deine Rolle nicht freigegeben',noWorkHere:'Keine Aufgaben'}};
  const context={labels,workspaceDataState,ctoxCollection:reads.ctoxCollection,taskSourceReadiness:()=>null,
    escapeHtml:x=>x,escapeAttr:x=>x,filterAndSortTasks:x=>x};
  const helpers=Function(...Object.keys(context),['dataState','dataStatusMarkup','taskListInner'].map(n=>body(n)).join('\n')+'\nreturn {dataState,dataStatusMarkup,taskListInner};')(...Object.values(context));
  const state={lang:'de',dataLoaded:true,model:{tasks:[]},taskSourceUnavailable:new Map([['ctox_queue_tasks',denied('ctox_queue_tasks')]])};
  assert.equal(helpers.dataState(state).kind,'restricted');
  const html=helpers.taskListInner([],state);assert.match(html,/data-ctox-data-state="restricted"/);
  assert.match(html,/für deine Rolle nicht freigegeben/);assert.doesNotMatch(html,/ctox-syncing|data-ctox-retry-load|Keine Aufgaben/);
  assert.equal(helpers.dataState({...state,dataError:'disk failed'}).kind,'error','unexpected errors retain precedence over a known denial');
});

const windowSource=body('openWindowedModule',shell);
const failure=windowSource.slice(windowSource.indexOf('  } catch (error) {\n    if (isRecoverableDataPlaneAbort(error))'),windowSource.indexOf('\n  } finally {'));
assert(failure.startsWith('  } catch'));
function windowFailure(error) {
  const errors=[],alerts=[],opened=[];const content={replaceChildren:node=>alerts.push(node)},root={dataset:{}};
  const document={createElement:()=>({dataset:{},querySelector:()=>({addEventListener:(_event,fn)=>opened.push(fn)})})};
  const scope={error,mod:{id:'notes'},win:{id:'notes-window'},root,content,options:{},state:{},
    els:{host:{replaceChildren:()=>assert.fail('another module host must stay intact')}},document,shellLang:()=> 'de',escapeHtml:x=>x,
    openAppLifecycleDrawer:mod=>opened.push(mod.id),isRecoverableDataPlaneAbort:()=>false,window:{location:{search:'?rxdbSmoke=1'}},
    URLSearchParams,moduleSyncLeasePromise:Promise.resolve(null),moduleDisplayTitle:mod=>mod.id,
    console:{error:(...args)=>errors.push(args)},renderWindowAppRecovery:()=>alerts.push('recovery'),
    closeWindowForRecovery:async()=>{},openWindowedModule:()=>assert.fail('no permission retry')};
  const result=Function(...Object.keys(scope),body('isBusinessOsPermissionError',shell)+'\n'+body('renderModulePermissionDeniedState',shell)+'\nreturn (async()=>{try {throw error;\n'+failure+'\n  }})();')(...Object.values(scope));
  return {result,errors,alerts,opened,root,state:scope.state};
}
for(const collection of ['iot_realms','notes'])test(`windowed ${collection} denial exposes permissions without mount-failure diagnostics`, async()=>{
  const error=Object.assign(new Error('Kein Leserecht'),{code:'CTOX_BUSINESS_OS_PERMISSION_DENIED',details:{collection,permission:'data.read'}});
  const f=windowFailure(error);assert.equal(await f.result,'notes-window');assert.deepEqual(f.errors,[]);
  assert.equal(f.root.dataset.modulePermissionDenied,'true');assert.equal(f.root.dataset.moduleLoadFailed,undefined);
  assert.equal(f.state.qaModuleMountFailures,undefined);assert.equal(f.alerts[0].dataset.collection,collection);
  assert.match(f.alerts[0].innerHTML,/Datenzugriff fehlt|App-Rechte ansehen/);f.opened[0]();assert.equal(f.opened[1],'notes');
});
test('unexpected window mount errors keep visible recovery, console error and exact QA details',async()=>{
  const error=new Error('broken app');const f=windowFailure(error);await f.result;
  assert.equal(f.errors.length,1);assert.equal(f.alerts[0],'recovery');assert.equal(f.root.dataset.moduleLoadFailed,'true');
  assert.equal(f.state.qaModuleMountFailures.notes.message,'broken app');
});

test('App Store target metadata and owning window take precedence over the desktop route',()=>{
  const state={modules:[{id:'desktop'},{id:'app-store'},{id:'target-app'}],activeModule:{id:'desktop'}};
  const resolve=Function('state',body('moduleForGlobalCtoxContextTarget',shell)+'\nreturn moduleForGlobalCtoxContextTarget;')(state);
  const target=id=>({closest:selector=>selector==='[data-context-module-id]'?{dataset:{contextModuleId:id}}:
    selector==='.shell-window'?{dataset:{ownerId:'desktop-app:app-store'}}:{dataset:{moduleRoot:'desktop'}}});
  assert.equal(resolve(target('target-app')).id,'target-app');
  assert.equal(resolve(target('unknown-tampered-id')).id,'app-store');assert.equal(resolve(target('')).id,'app-store');
});

test('a private app route creates no window and its visible denial survives a desktop fallback',async()=>{
  const alerts=[];const desktop={id:'desktop'},hidden={id:'private-app',title:'Secret title'};
  const scope={state:{modules:[desktop,hidden],activeModule:desktop},moduleAliases:{},parseHashWithParams:name=>({name}),searchParamsToObject:()=>({}),
    currentHashModuleId:()=>'',canSeeModuleForAppVersion:mod=>mod.id==='desktop',appLifecycleState:()=>({reason:'private'}),
    visibleModuleFallbackId:()=> 'desktop',moduleDisplayTitle:mod=>mod.title||mod.id,setStatus:()=>{},shellLang:()=> 'de',
    showBusinessAlert:text=>alerts.push(text),moduleLaunchesAsDesktopApp:()=>false,openDesktopApp:()=>assert.fail('private app must not open')};
  const open=Function(...Object.keys(scope),body('openModule',shell)+'\nreturn openModule;')(...Object.values(scope));
  await open('private-app');assert.equal(alerts.length,1);assert.match(alerts[0],/nicht sichtbar/);assert.doesNotMatch(alerts[0],/Secret title|private-app/);
});
