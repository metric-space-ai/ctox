#!/usr/bin/env python3
"""E13: disable each real E25 route, run the real Kernel/P9 contract, restore.
Run ONLY through the canonical lane. No remote/profile credentials are read.
"""
import argparse, hashlib, json, os, pathlib, re, subprocess, time, signal, datetime, shutil
p=argparse.ArgumentParser();p.add_argument('--out',type=pathlib.Path,required=True);p.add_argument('--only',action='append');p.add_argument('--routes-only',action='store_true');p.add_argument('--per-method',action='store_true');p.add_argument('--deadline');p.add_argument('--cache-baseline',action='store_true');a=p.parse_args()
deadline=datetime.datetime.fromisoformat(a.deadline.replace('Z','+00:00')).timestamp() if a.deadline else float('inf')
def interrupted(_signal,_frame): raise SystemExit(130)
signal.signal(signal.SIGTERM,interrupted)
a.out.mkdir(parents=True,exist_ok=True)
root=pathlib.Path.cwd();source=root/'src/core/store/command_routes.rs'
original=source.read_bytes();digest=hashlib.sha256(original).hexdigest()
inventory=json.loads((root/'tests/sync_v4_e25/e25-methods.json').read_text())
items=inventory['methods']+inventory.get('command_bus',[])
if a.only:
    assert set(a.only)<=set(x['method'] for x in items)
    items=[x for x in items if x['method'] in a.only]
command=['cargo','test','--locked','-p','ctox-store','e25_kernel_command_to_native_completion','--','--nocapture']
def run(name, selected=command, method=None):
    if time.time()>=deadline: raise TimeoutError('E25 qualification deadline')
    with (a.out/(name+'.log')).open('w') as log:
        child=subprocess.Popen(selected,stdout=log,stderr=subprocess.STDOUT,start_new_session=True,env={**os.environ,'RUST_TEST_THREADS':'2','CTOX_SKIP_OPTIONAL_RUNTIME_BUILDS':'1',**({'E25_METHOD':method} if method else {})})
        receipt['active']={'pid':child.pid,'pgid':child.pid,'command':selected,'stop_condition':'stage timeout, signal or qualification deadline'}
        (a.out/'mutation.json').write_text(json.dumps(receipt,indent=2)+'\n')
        try: code=child.wait(timeout=max(1,min(720,deadline-time.time())))
        except BaseException:
            if child.poll() is None:
                os.killpg(child.pid,signal.SIGTERM)
                try:child.wait(timeout=5)
                except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
            raise
        finally:receipt.pop('active',None)
    return code,(a.out/(name+'.log')).read_text()
receipt={'pass':False,'selected_methods':a.only,'per_method':a.per_method,'routes_only':a.routes_only,'source_sha256':digest,'command':command,'profile':'component, fixture DTOs; not M7/product acceptance','methods':[]}
try:
    baseline_command=command
    if a.cache_baseline:
        # Keep the immutable, source-marked executable, not mutable Cargo output.
        # Every mutant still rebuilds the actual production route and must fail.
        code,build=run('compile-baseline',['cargo','test','--locked','-p','ctox-store','--no-run','--message-format=json'])
        assert code==0,build[-4000:]
        artifacts=[]
        for line in build.splitlines():
            try: artifact=json.loads(line)
            except ValueError: continue
            if artifact.get('reason')=='compiler-artifact' and artifact.get('target',{}).get('name')=='ctox_store' and artifact.get('profile',{}).get('test') and artifact.get('executable'):
                artifacts.append(artifact['executable'])
        assert len(set(artifacts))==1,artifacts
        binary=a.out.resolve()/'immutable-baseline'
        shutil.copy2(artifacts[0],binary)
        code,marker=run('baseline-source-marker',[str(binary),'e25_all_routes_complete_without_changing_method_payload_or_command_id','--nocapture'])
        assert code==0 and 'E25_REGISTRY_SOURCE_SHA256 '+digest in marker,marker[-4000:]
        receipt['immutable_baseline']={'sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'registry_sha256':digest,'marker_log':'baseline-source-marker.log'}
        baseline_command=[str(binary),'e25_kernel_command_to_native_completion','--nocapture']
    baseline,text=run('baseline',baseline_command,method=items[0]['method'] if a.only or a.per_method else None);assert baseline==0, text[-4000:]
    assert 'E25_KERNEL_COMPONENT' in text,'missing actual component execution'
    marker='E25_KERNEL_COMPONENT ';baseline_profile=json.loads(text[text.rindex(marker)+len(marker):].splitlines()[0])
    assert baseline_profile['pass'] and baseline_profile['completion_lanes']==[1]
    receipt['baseline_scope']='fresh Store per route; each measured before its mutation' if a.only or a.per_method else 'shared full-inventory Kernel profile; immutable source SHA256'
    receipt['baseline_log_sha256']=hashlib.sha256(text.encode()).hexdigest()
    for item in items:
        method=item['method'];pattern=r'\s*(?:core_)?route!\(\s*"'+re.escape(method)+r'"\s*,\s*"[^"]*"\s*(?:,\s*(?:true|false))?\s*,?\s*\),'
        # One full, successful profile executed every method with identical
        # immutable source bytes. Reuse that measured baseline, not another
        # compile of the same restored source before every single route mutant.
        if a.only or a.per_method:
            baseline,text=run('baseline_'+method.replace('.','_'),baseline_command,method=method);assert baseline==0,text[-4000:]
            marker='E25_KERNEL_COMPONENT ';baseline_profile=json.loads(text[text.rindex(marker)+len(marker):].splitlines()[0])
        assert any(m['method']==method and m['pass'] for m in baseline_profile['methods']),method
        candidate,n=re.subn(pattern,'',original.decode());assert n==1,(method,n)
        source.write_text(candidate);os.utime(source,None)
        code,log=run(method.replace('.','_'),method=method)
        assert code!=0 and 'unknown_command' in log and 'FAILED' in log and 'could not compile' not in log,(method,code,log[-3000:])
        receipt['methods'].append({'method':method,'mutation':'remove complete route! entry','baseline_exit':baseline,'baseline_log':('baseline_'+method.replace('.','_')+'.log') if a.only or a.per_method else 'baseline.log','mutant_exit':code,'reason':'unknown_command','test':'e25_kernel_command_to_native_completion'})
        source.write_bytes(original);os.utime(source,None)
        (a.out/'mutation.json').write_text(json.dumps(receipt,indent=2)+'\n')
    # Decisive security/transaction invariants, not extra acceptance gates.
    guards=[
        ('secret-not-durable',source,b'if contains_credential(&document["payload"])',b'if false','e25_raw_secret_never_enters_order_buffer_or_command_history','store'),
        ('files-and-completion-atomic',root/'src/core/rxdb/src/plugins/replication_webrtc/v4_native_commands.rs',b'ROLLBACK TO e25_completion; RELEASE e25_completion',b'RELEASE e25_completion','e25_verified_cas_stages_off_writer_and_failed_completion_rolls_back_files','rxdb'),
    ]
    native=root/'src/core/rxdb/src/plugins/replication_webrtc/v4_native_commands.rs'
    guards += [
        ('stream-owner',source,b'if owns_stream {',b'if route.streaming {','e25_stream_partials_keep_the_command_id_and_never_finish_on_open','store'),
        ('control-lane',source,b'pub const CONTROL_LANE: u8 = 1;',b'pub const CONTROL_LANE: u8 = 2;','e25_kernel_command_to_native_completion','store'),
        ('partial-not-terminal',source,b'("running", json!({"partial":native}))',b'("done", json!({"partial":native}))','e25_stream_partials_keep_the_command_id_and_never_finish_on_open','store'),
        ('files-ref-required',source,b'if bytes.len() > INLINE_RESULT_LIMIT || contains_inline_audio(&value) {',b'if false {','e25_large_and_audio_results_require_verified_files_ref_in_same_transaction','store'),
        ('protected-result',source,b'return Err(error("unavailable", "protected_result_adapter_required"));',b'return Ok(Completion{command_id:command.command_id.clone(),status:"done".into(),result:Some(native),error:None});','e25_protected_control_results_never_become_command_or_files_payloads','store'),
        ('typed-native-error',native,b'.map_err(|s| routes::native_error(&s))',b'.map_err(|_| routes::error("unavailable","native_handler_failed"))','e25_native_typed_error_reaches_completion_without_coarse_rethrow','rxdb'),
        ('native-publication',native,re.compile(rb'answer\s*\.publication\s*\.unwrap_or_else\(\|\| Arc::new\(ExistingPeerAuthority\)\)'),b'Arc::new(ExistingPeerAuthority) as Arc<dyn WebRTCPublicationGuard>','e25_native_publication_guard_survives_the_command_adapter','rxdb'),
        ('peer-revocation',native,re.compile(rb'let publication = Arc::new\(AuxiliaryPublicationGuard \{[^;]*?\n\s*native,\n\s*\}\);',re.S),b'let publication = Arc::new(ExistingPeerAuthority);','e25_revocation_between_execution_and_completion_cannot_publish','rxdb'),
        ('actor-admission',native,re.compile(rb'(?P<prefix>let mut future = None;\s*)authority\s*\.with_current\(&command, &peer_identity, &capability, &mut \|\| \{(?P<body>.*?)\n\s*\}\)\n\s*\.map_err\(\|_\| routes::error\("forbidden", "command_authority_changed"\)\)\?;',re.S),rb'\g<prefix>(|| {\g<body>\n})().map_err(|_| routes::error("forbidden","command_authority_changed"))?;','e25_adapter_reuses_registered_handler_and_retains_original_actor','rxdb'),
    ]
    guards += [
        ('unknown-before-write',root/'src/core/store/write_contract.rs',b'routes::owns_type(kind)',b'false','e25_workjet_inventory_is_registered_before_write','store'),
        ('session-peer-binding',root/'src/core/rxdb/src/plugins/replication_webrtc/v4_native_sessions.rs',b'if parent.peer != *peer',b'if false','e25_speech_session_controls_and_native_events_keep_one_opening_identity','rxdb'),
        ('child-measurement-required',root/'src/core/store/command_routes_e2e.mjs',b"await new Promise((resolve,reject)=>process.stdout.write(JSON.stringify({type:'measured'",b"process.exit(0);await new Promise((resolve,reject)=>process.stdout.write(JSON.stringify({type:'measured'",'e25_kernel_command_to_native_completion','store'),
        ('cas-chunk-hash',root/'src/core/rxdb/src/plugins/replication_webrtc/v4_transfer.rs',b'Sha256::digest(&data)',b'Sha256::digest(b"wrong chunk")','e25_native_cold_range_sends_complete_hashed_chunks_and_fences_revocation','rxdb'),
    ]
    guards += [
        ('canonical-actor',source,b'doc["actor_id"] == command.actor_id',b'true','e25_session_terminal_replay_is_derived_from_the_same_canonical_snapshot','store'),
        ('native-root-actor',root/'src/core/business_os/command_plane_v4_routes.rs',b'if session_user_id(&session) != Some(native.actor_id.as_str())',b'if false','e25_native_command_bus_preserves_user_and_skips_legacy_intake','root'),
        ('speech-native-notification',root/'src/core/business_os/rxdb_peer_dictation.rs',re.compile(rb'(?:self\.)?updates\.send_replace\(\(\)\)'),b'()','verified_final_is_correlated_draft_text_without_domain_mutations','root'),
    ]
    guards += [
        ('linked-cas-stage',native,b'current = pending.linked.as_deref();',b'current = None;','e25_linked_results_stage_distinct_verified_cas_digests_before_one_commit','rxdb'),
    ]
    receipt['guards']=[]
    for name,path,old,new,test,crate in ([] if a.routes_only else guards):
        clean=path.read_bytes()
        if isinstance(old,bytes):
            count=clean.count(old);candidate=clean.replace(old,new)
        else:
            candidate,count=old.subn(new,clean)
        assert (count>=4 if name=='speech-native-notification' else count==(2 if name=='typed-native-error' else 1)),(name,'anchor',count)
        try:
            path.write_bytes(candidate);os.utime(path,None)
            selected=['cargo','test','--locked']+(['-p','ctox-store'] if crate=='store' else ['-p','ctox','--bin','ctox'] if crate=='root' else ['--manifest-path','src/core/rxdb/Cargo.toml','--lib'])+[test,'--','--nocapture']
            code,log=run(name,selected)
            assert code!=0 and 'FAILED' in log and 'could not compile' not in log,(name,code,log[-3000:])
            receipt['guards'].append({'invariant':name,'test':test,'mutant_exit':code,'production_anchor':old.decode() if isinstance(old,bytes) else old.pattern.decode()})
        finally:
            path.write_bytes(clean);os.utime(path,None)
    restored,text=run('restored',method=items[0]['method'] if a.only or a.per_method else None);assert restored==0,text[-4000:]
    native_command=['cargo','test','--locked','--manifest-path','src/core/rxdb/Cargo.toml','--lib','e25_','--','--nocapture']
    native_restored,native_log=run('native-restored',native_command);assert native_restored==0,native_log[-4000:]
    assert 'E25_ADAPTER_SOURCE_SHA256' in native_log,'missing rebuilt native adapter marker'
    for label,filter in ([] if a.routes_only else [('root-restored','e25_native_command_bus_preserves_user'),('speech-restored','verified_final_is_correlated_draft_text')]):
        code,log=run(label,['cargo','test','--locked','-p','ctox','--bin','ctox',filter,'--','--nocapture'])
        assert code==0 and 'test result: ok.' in log,(label,code,log[-3000:])
        receipt[label+'_exit']=code
    receipt['native_restored_exit']=native_restored
    receipt['native_source_sha256']=hashlib.sha256((root/'src/core/rxdb/src/plugins/replication_webrtc/v4_native_commands.rs').read_bytes()).hexdigest()
    receipt.update({"pass":True,"restored_exit":restored,"restored_sha256":hashlib.sha256(source.read_bytes()).hexdigest()})
finally:
    source.write_bytes(original);os.utime(source,None)
    receipt['restored_source_matches']=hashlib.sha256(source.read_bytes()).hexdigest()==digest
    (a.out/'mutation.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
