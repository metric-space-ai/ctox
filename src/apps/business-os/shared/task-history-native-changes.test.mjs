import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { subscribeTaskHistoryChanges } from './task-history-native-changes.js';
import { CollectionSyncRegistry } from './sync-collection-registry.js';

const settle = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
function nativeState(name) {
  const listeners = new Set();
  return { collection: { name }, masterChange$: { subscribe(fn) { listeners.add(fn); return { unsubscribe() { listeners.delete(fn); } }; } },
    emit(hint) { for (const fn of [...listeners]) fn(hint); }, listeners };
}
function setup() {
  const registry = new CollectionSyncRegistry();
  const states = new Map(['ctox_runs', 'ctox_harness_events'].map(name => [name, nativeState(name)]));
  for (const [name, state] of states) registry.set(name, Promise.resolve({ state }));
  const sync = { leaseCollection: async name => registry.acquire(name, 'test', async () => {}) };
  return { registry, states, sync };
}

test('selected native histories trigger unique authoritative reads; other tasks are ignored', async () => {
  const f = setup(); const changes = [];
  let selection = { key: 'a', taskId: 'a', commandId: 'cmd-a' };
  const stop = subscribeTaskHistoryChanges({ sync: f.sync, getSelection: () => selection, onChange: value => changes.push(value) });
  await settle();
  f.states.get('ctox_runs').emit({ documents: [{ task_id: 'other' }] });
  f.states.get('ctox_runs').emit({ documents: [{ task_id: 'a' }] });
  f.states.get('ctox_harness_events').emit({ result: { documents: [{ documentData: { command_id: 'cmd-a' } }] } });
  assert.equal(changes.length, 2); assert.notEqual(changes[0].revision, changes[1].revision);
  assert(changes.every(value => value.key === 'a' && value.taskId === 'a'));
  selection = { key: 'b', taskId: 'b', commandId: 'cmd-b' };
  f.states.get('ctox_runs').emit({ documents: [{ task_id: 'a' }] });
  f.states.get('ctox_runs').emit({ documents: [{ task_id: 'b' }] });
  assert.equal(changes.length, 3); assert.equal(changes[2].key, 'b'); stop(); await settle();
});

test('legacy hints refresh only a selected task; empty and unselected hints do no work', async () => {
  const f = setup(); let selection = null; const changes = [];
  const stop = subscribeTaskHistoryChanges({ sync: f.sync, getSelection: () => selection, onChange: value => changes.push(value) });
  await settle(); f.states.get('ctox_runs').emit(null); assert.equal(changes.length, 0);
  selection = { taskId: 'a', key: 'a' };
  f.states.get('ctox_runs').emit({ documents: [] }); assert.equal(changes.length, 0);
  f.states.get('ctox_runs').emit(null); assert.equal(changes.length, 1); stop(); await settle();
});

test('bridge replacement retires old listeners and close releases both leases', async () => {
  const f = setup(); let changes = 0;
  const stop = subscribeTaskHistoryChanges({ sync: f.sync, getSelection: () => ({ taskId: 'a' }), onChange: () => changes++ });
  await settle(); const old = f.states.get('ctox_runs');
  const current = nativeState('ctox_runs'); f.registry.set('ctox_runs', Promise.resolve({ state: current })); await settle();
  assert.equal(old.listeners.size, 0); old.emit(null); assert.equal(changes, 0);
  current.emit(null); assert.equal(changes, 1); stop(); stop(); await settle();
  current.emit(null); assert.equal(changes, 1);
  assert.equal(current.listeners.size, 0); assert.equal(f.states.get('ctox_harness_events').listeners.size, 0);
  assert.equal(f.registry.leaseCount('ctox_runs'), 0); assert.equal(f.registry.leaseCount('ctox_harness_events'), 0);
});

test('late lease acquisition is released after close without binding a listener', async () => {
  const f = setup(); const resolve = []; let changes = 0;
  const sync = { leaseCollection: name => new Promise(yes => resolve.push(() => yes(f.registry.acquire(name, 'late', async () => {})))) };
  const stop = subscribeTaskHistoryChanges({ sync, getSelection: () => ({ taskId: 'a' }), onChange: () => changes++ });
  await settle(); assert.equal(resolve.length, 2); stop(); resolve.forEach(yes => yes()); await settle();
  for (const [name, state] of f.states) { assert.equal(f.registry.leaseCount(name), 0); assert.equal(state.listeners.size, 0); state.emit(null); }
  assert.equal(changes, 0);
});

test('permission denial remains a denied lease, with no fallback or collection write', async () => {
  const calls = []; const errors = [];
  const sync = { async leaseCollection(name) { calls.push(name); throw new Error('denied'); } };
  const stop = subscribeTaskHistoryChanges({ sync, getSelection: () => ({ taskId: 'a' }), onChange: () => assert.fail('denied hint'), onError: e => errors.push(e) });
  await settle(); assert.equal(errors.length, 2); assert.deepEqual(calls, ['ctox_runs', 'ctox_harness_events']); stop();
});

// The override is test-only: it runs these behavioral guards against an exact
// historical source fixture for the negative control without changing source.
const source = readFileSync(process.env.CTOX_HISTORY_TEST_SOURCE || new URL('../modules/ctox/index.js', import.meta.url), 'utf8');
function body(name) {
  const from = source.indexOf(`function ${name}(`); assert(from >= 0);
  const start = source.slice(Math.max(0, from - 6), from) === 'async ' ? from - 6 : from;
  return source.slice(start, source.indexOf('\n}', from) + 2);
}

test('actual cockpit coalesces native selected-task hints and retires them on close', async () => {
  const f = setup(); const selected = { id: 'a', commandId: 'cmd-a' }; const reads = [];
  const state = { ctx: { sync: f.sync }, disposed: false };
  const wire = Function('ctoxCollection', 'getSelectedTask', 'nativeTaskId', 'taskLiveKey', 'refreshConfirmedHarnessStatus', 'renderFromLocalCache', 'subscribeTaskHistoryChanges', 'window', 'LOCAL_RENDER_DEBOUNCE_MS', body('wireLocalRealtime')+'\nreturn wireLocalRealtime;')(
    () => ({ $: { subscribe() { return { unsubscribe() {} }; } } }), () => selected, task => task?.id || '', task => task?.id || '', () => {}, async state => reads.push(state.taskHistoryRevision), subscribeTaskHistoryChanges, { setTimeout, clearTimeout }, 10);
  const stop = wire(state); await settle();
  for (let i = 0; i < 20; i++) f.states.get('ctox_harness_events').emit({ documents: [{ task_id: 'a' }] });
  await new Promise(yes => setTimeout(yes, 30));
  assert.equal(reads.length, 1, 'the actual cockpit must re-read after a native history hint');
  assert.equal(reads[0].key, 'a'); assert(reads[0].revision);
  stop(); await settle(); f.states.get('ctox_harness_events').emit(null);
  await new Promise(yes => setTimeout(yes, 30)); assert.equal(reads.length, 1);
  assert.equal(f.registry.leaseCount('ctox_runs'), 0); assert.equal(f.registry.leaseCount('ctox_harness_events'), 0);
});

test('actual selected-task reads retain task bounds and authoritative revision through sort fallback', async () => {
  const queries = []; const collections = Object.fromEntries(['ctox_runs', 'ctox_harness_events'].map(name => [name, {
    find(query) { queries.push({ name, query }); return { async exec() { if (query.sort) throw new Error('unsupported sort'); return [{ toJSON: () => ({ id: name, task_id: 'a' }) }]; } }; },
  }]));
  const load = Function('ctoxCollection', 'nativeTaskId', 'taskLiveKey', 'harnessFlowFromEvents', 'HARNESS_EVENT_LIMIT', ['findLocalDocs','loadLocalHarnessEvents','loadLocalRunsForTask','loadSelectedTaskLive'].map(body).join('\n')+'\nreturn loadSelectedTaskLive;')(
    (_ctx, name) => collections[name], task => task.id, task => task.id, () => null, 200);
  const result = await load({}, { id: 'a', commandId: 'cmd-a' }, 'authoritative-history-1');
  assert.equal(result.events.length, 1); assert.equal(result.runs.length, 1); assert.equal(queries.length, 4);
  for (const { name, query } of queries) {
    assert.deepEqual(query.selector, { task_id: 'a' }); assert.equal(query.limit, name === 'ctox_runs' ? 32 : 200);
    assert.equal(query.requireRevision, 'authoritative-history-1');
  }
});

function hydrationHarness(state, getSelected, load) {
  const noop = () => {};
  const scope = {
    refreshConfirmedHarnessStatus: noop, loadLocalCommands: async () => [], loadLocalQueueTasks: async () => [],
    loadLocalBugReports: async () => [], loadLocalWebStackOverview: async () => ({ok:true}),
    loadHarnessFlowSnapshot: async () => ({ok:true}), emptyHarnessFlow: () => ({}),
    loadLocalCrewMembers: async () => [], loadLocalChannelAccounts: async () => [], armExpressionRefresh: noop,
    mergeBundleWithCommands: () => ({}), ctoxSeed: {}, buildHarnessModel: () => ({}), publishCrewWorkload: noop,
    readFocusTask: () => null, reconcileSelection: noop, getSelectedTask: getSelected,
    taskLiveKey: task => task?.id || '', loadSelectedTaskLive: load,
    applyLiveFlow: () => { state.paints++; }, deriveHarnessHealth: () => ({}), displayFlowMode: value => value,
    render: noop, syncDetailDrawer: noop,
  };
  return Function(...Object.keys(scope), body('hydrateFromLocal')+'\nreturn hydrateFromLocal;')(...Object.values(scope));
}

test('actual hydration cannot paint the previous selection after an in-flight history read', async () => {
  let selected = { id: 'a' }; let finish;
  const state = { ctx:{sync:{mode:'webrtc'}}, paints:0, taskHistoryRevision:{key:'a',revision:'force-a'} };
  const revisions = [];
  const hydrate = hydrationHarness(state, () => selected, (_ctx, _task, revision) => {
    revisions.push(revision); return new Promise(yes => { finish = yes; });
  });
  const reading = hydrate(state); await settle(); assert.equal(typeof finish, 'function');
  selected = { id:'b' }; state.taskHistoryRevision = {key:'b',revision:'force-b'};
  finish({key:'a', events:[], runs:[]}); await reading;
  assert.equal(state.paints,0); assert.equal(state.selectedLive,undefined);
  assert.equal(state.rerenderAfterRefresh,true); assert.equal(state.taskHistoryRevision.key,'b');
  assert.deepEqual(revisions,['force-a']);
});

test('a newer native hint survives an in-flight hydrate and clears only after its own bounded read', async () => {
  let finish; const selected = {id:'a'}; const revisions = [];
  const state = {ctx:{sync:{mode:'webrtc'}},paints:0,taskHistoryRevision:{key:'a',revision:'first'}};
  const hydrate = hydrationHarness(state, () => selected, (_ctx, _task, revision) => {
    revisions.push(revision);
    if (revisions.length === 1) return new Promise(yes => {finish=yes;});
    return Promise.resolve({key:'a',events:[],runs:[]});
  });
  const reading=hydrate(state); await settle(); assert.equal(typeof finish,'function');
  const newer={key:'a',revision:'second'}; state.taskHistoryRevision=newer;
  finish({key:'a',events:[],runs:[]}); await reading;
  assert.equal(state.taskHistoryRevision,newer);
  await hydrate(state); assert.deepEqual(revisions,['first','second']); assert.equal(state.taskHistoryRevision,null);
});


test('live role preflight skips denied collections and reports their unavailable state without leases', async () => {
  const calls = [], unavailable = [], errors = [];
  const sync = { mayReadCollection: () => false, leaseCollection: name => { calls.push(name); assert.fail('denied lease'); } };
  const stop = subscribeTaskHistoryChanges({ sync, getSelection: () => ({taskId:'a'}),
    onChange: () => assert.fail('denied hint'), onError: e => errors.push(e), onUnavailable: x => unavailable.push(x) });
  await settle();
  assert.deepEqual(calls, []); assert.deepEqual(errors, []);
  assert.deepEqual(unavailable.map(x => x.collection), ['ctox_runs', 'ctox_harness_events']);
  assert(unavailable.every(x => x.error.code === 'COLLECTION_READ_FORBIDDEN'));
  stop(); stop();
});

test('a role change during lease acquisition releases late leases without subscribing or fallback', async () => {
  const f=setup(), pending=[], unavailable=[], errors=[]; let readable=true;
  const sync={mayReadCollection:()=>readable, leaseCollection:name=>new Promise(yes=>pending.push(()=>yes(f.registry.acquire(name,'race',async()=>{}))))};
  const stop=subscribeTaskHistoryChanges({sync,getSelection:()=>({taskId:'a'}),onChange:()=>assert.fail('retired hint'),
    onError:e=>errors.push(e),onUnavailable:x=>unavailable.push(x)});
  await settle(); assert.equal(pending.length,2); readable=false; pending.forEach(resolve=>resolve()); await settle();
  assert.equal(unavailable.length,2); assert.deepEqual(errors,[]);
  for(const [name,state] of f.states){assert.equal(f.registry.leaseCount(name),0);assert.equal(state.listeners.size,0);}
  stop(); await settle();
});

test('coded native role denial retires the observation but uncoded denial remains an error', async () => {
  const unavailable=[], errors=[], calls=[];
  const denied=new Error('policy denied');denied.code='COLLECTION_READ_FORBIDDEN';
  const unexpected=new Error('COLLECTION_READ_FORBIDDEN text alone is not the typed contract');
  const sync={mayReadCollection:()=>true,async leaseCollection(name){calls.push(name);throw name==='ctox_runs'?denied:unexpected;}};
  const stop=subscribeTaskHistoryChanges({sync,getSelection:()=>({taskId:'a'}),onChange:()=>assert.fail('denied hint'),
    onUnavailable:x=>unavailable.push(x),onError:e=>errors.push(e)});
  await settle();assert.deepEqual(calls,['ctox_runs','ctox_harness_events']);
  assert.equal(unavailable.length,1);assert.equal(unavailable[0].error,denied);assert.deepEqual(errors,[unexpected]);stop();
});

test('revoked histories cannot publish a hint and release both old and replacement subscriptions', async () => {
  const f=setup(), unavailable=[], errors=[], changes=[];let readable=true;
  f.sync.mayReadCollection=()=>readable;
  const options={sync:f.sync,getSelection:()=>({taskId:'a'}),onChange:x=>changes.push(x),
    onUnavailable:x=>unavailable.push(x),onError:e=>errors.push(e)};
  const stop=subscribeTaskHistoryChanges(options);await settle();readable=false;
  for(const state of f.states.values()){state.emit(null);state.emit(null);}await settle();
  assert.equal(changes.length,0);assert.equal(unavailable.length,2);assert.deepEqual(errors,[]);
  for(const [name,state] of f.states){assert.equal(state.listeners.size,0);assert.equal(f.registry.leaseCount(name),0);}
  const replacement=nativeState('ctox_runs');f.registry.set('ctox_runs',Promise.resolve({state:replacement}));await settle();
  assert.equal(replacement.listeners.size,0);replacement.emit(null);assert.equal(changes.length,0);stop();
  readable=true;const fresh=subscribeTaskHistoryChanges(options);await settle();replacement.emit(null);
  assert.equal(changes.length,1,'a newly authorized observation can acquire a fresh lease');fresh();await settle();
});

test('unexpected lease errors stay visible and releases are still bounded after a callback failure', async () => {
  const f=setup(), errors=[];const unexpected=new Error('selection unavailable');
  const stop=subscribeTaskHistoryChanges({sync:f.sync,getSelection:()=>{throw unexpected;},onChange:()=>assert.fail('unexpected hint'),onError:e=>errors.push(e)});
  await settle();f.states.get('ctox_runs').emit(null);await settle();
  assert.deepEqual(errors,[unexpected]);assert.equal(f.registry.leaseCount('ctox_runs'),0);
  assert.equal(f.states.get('ctox_runs').listeners.size,0);stop();await settle();assert.equal(f.registry.leaseCount('ctox_harness_events'),0);
});

test('actual cockpit exposes denied history access without warnings or claiming its data is readable', async () => {
  const calls=[], state={ctx:{sync:{mayReadCollection:()=>false,leaseCollection:()=>assert.fail('denied native lease')}},lang:'de',disposed:false};
  const wire=Function('ctoxCollection','getSelectedTask','nativeTaskId','taskLiveKey','refreshConfirmedHarnessStatus','renderFromLocalCache','subscribeTaskHistoryChanges','showDataError','window','LOCAL_RENDER_DEBOUNCE_MS',body('wireLocalRealtime')+'\nreturn wireLocalRealtime;')(
    ()=>null,()=>({id:'a'}),t=>t.id,t=>t.id,()=>{},async()=>calls.push('render'),subscribeTaskHistoryChanges,()=>assert.fail('role denial is not an unexpected data error'),{setTimeout,clearTimeout},1);
  const stop=wire(state);await settle();await new Promise(yes=>setTimeout(yes,10));
  assert.deepEqual([...state.taskHistoryUnavailable],['ctox_runs','ctox_harness_events']);assert.deepEqual(calls,['render']);
  const notice=Function('labels','escapeAttr','escapeHtml',body('taskHistoryPermissionNotice')+'\nreturn taskHistoryPermissionNotice;')(
    {de:{timeline:'Verlauf',notPermittedForRole:'für deine Rolle nicht freigegeben'}},x=>x,x=>x);
  assert.match(notice(state),/data-task-history-unavailable role="status"/);assert.match(notice(state),/COLLECTION_READ_FORBIDDEN: ctox_runs, ctox_harness_events/);
  assert.match(notice(state),/Verlauf: für deine Rolle nicht freigegeben/);assert.equal(notice({...state,taskHistoryUnavailable:new Set()}),'');stop();await settle();
});
