#!/usr/bin/env python3
"""CAP-3 contract mutation proofs. Run only through the GPU build lane.
usage: cap3_mutations.py [deadline-iso] [label,label,...] [--restore-hash-only]"""
import datetime, hashlib, json, os, pathlib, re, signal, subprocess, sys, time
ROOT=pathlib.Path.cwd()
SOURCE=ROOT/"src/core/app_capabilities/caps"
OUT=pathlib.Path(os.environ["CARGO_TARGET_DIR"])/("cap3-proofs-"+datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ"))
OUT.mkdir(parents=True,exist_ok=False)
DEADLINE=datetime.datetime.fromisoformat(sys.argv[1]).timestamp() if len(sys.argv)>1 and sys.argv[1] else time.time()+5400
STORE="../../business_os/store.rs"
PEER="../../business_os/rxdb_peer_browser.rs"
FILES=[SOURCE/name for name in ["cap3.rs","audit.rs","config.rs","secrets.rs","lock.rs","flow.rs","browser.rs","ingest.rs","coding.rs","mail.rs","directory.rs","files.rs","process.rs","cap3_tests.rs",STORE,PEER]]+[SOURCE.parent/"mod.rs",SOURCE/"mod.rs"]
ORIGINAL={path:path.read_bytes() for path in FILES}
def digest():
    return {str(path.relative_to(ROOT)):hashlib.sha256(path.read_bytes()).hexdigest() for path in FILES}
original_digest=digest()
(OUT/"source-sha256.json").write_text(json.dumps(original_digest,indent=2)+"\n")
print("CAP3_PROOF_DIR="+str(OUT),flush=True)
def run(label,selector,exact=False):
    remain=int(DEADLINE-time.time())
    if remain<120: raise RuntimeError("proof deadline reached; no next run started")
    args=["cargo","test","--locked","-p","ctox","--bin","ctox",selector,"--"]
    if exact: args+=["--exact"]
    args+=["--test-threads=1"]
    output=OUT/(label+".log")
    with output.open("w") as stream:
        process=subprocess.Popen(args,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
        print("RUN "+label+" pid="+str(process.pid)+" output="+str(output),flush=True)
        try: code=process.wait(timeout=min(1500,remain))
        except subprocess.TimeoutExpired:
            os.killpg(process.pid,signal.SIGTERM)
            try: process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid,signal.SIGKILL);process.wait()
            raise RuntimeError(label+" timed out")
    return code,output.read_text(errors="replace")
E=re.escape
PROOFS=[
 ("audit-scope","audit.rs",E("(?1 IS NULL OR json_extract(payload_json,'$._cap.app_id') = ?1)"),"(1=1)","audit_scope_contract"),
 ("config-whitelist","config.rs",E("if !ALLOWED_KEYS.contains(&request.key.as_str()) {"),"if false {","config_whitelist_contract"),
 ("secret-scope","secrets.rs",E('Some(_) => Err(forbidden("secret scope belongs to another owner")),'),"Some(other) => Ok(other.into()),","secrets_scope_contract"),
 ("secret-intake-binding","secrets.rs",E("if args.command_id.as_deref() != Some(ctx.command_id.as_str()) {"),"if false {","secrets_scope_contract"),
 ("lock-stale-fence","lock.rs",E("WHERE app_id=?1 AND fence=?2 AND token=?3"),"WHERE app_id=?1 AND ?2>0 AND ?3<>''","lock_fence_contract"),
 ("lock-ttl","lock.rs",E("let live = *expires > now;"),"let live = true;","lock_fence_contract"),
 ("lock-single-holder","lock.rs",E("if live {"),"if false {","lock_fence_contract"),
 ("flow-platform","flow.rs",E("if !is_platform(ctx) {"),"if false {","flow_platform_contract"),
 ("browser-op-allowlist","browser.rs",E("if !ALLOWED_OPS.contains(&request.op.as_str()) {"),"if false {","browser_device_contract"),
 ("browser-blobref","browser.rs",E('"blob":{"ref":meta.reference}'),'"blob":meta.reference.clone() + "transfer"',"browser_device_contract"),
 ("ingest-fallback-3s","ingest.rs",E("match receiver.recv_timeout(wait) {"),"match receiver.recv() {","ingest_fallback_contract"),
 ("ingest-header-whitelist","ingest.rs",r"if !RESPONSE_HEADERS\.contains\(&name\.as_str\(\)\) \{\s*continue;\s*\}","","ingest_header_contract"),
 ("ingest-request-private","ingest.rs",E("if PRIVATE_HEADERS.contains(&name.as_str()) && !route.pass_headers.contains(&name) {"),"if false {","ingest_header_contract"),
 ("ingest-location","ingest.rs",E("if !browser::web_url(location) {"),"if false {","ingest_location_contract"),
 ("ingest-body-ref","ingest.rs",E("let bytes = runtime.load_body(app_id, &reference).ok()?;"),"let bytes = reference.clone().into_bytes();","ingest_body_ref_contract"),
 ("ingest-unknown-command","ingest.rs",E("if !runtime.has_handler(&ctx.app_id, &request.command)? {"),"if false {","ingest_register_contract"),
 ("coding-flat","coding.rs",E("if object.keys().any(|key| !allowed.contains(&key.as_str())) {"),"if false {","coding_flat_contract"),
 ("mail-intake-binding","mail.rs",E("if request.command_id != ctx.command_id {"),"if false {","mail_server_intake_contract"),
 # ---- CAP-3b
 ("intake-field-binding",STORE,E('["value", "password"].into_iter().find(|f| *f == field)?'),'["value", "password"].into_iter().find(|_| true)?',"secret_intake_live_contract"),
 ("platform-origin","config.rs",E("if !is_platform(ctx) {"),"if false {","platform_origin_contract"),
 ("ingest-serves-alias","ingest.rs",E(".is_ok_and(|route| route.is_some())"),".is_ok()","platform_origin_contract"),
 ("files-owner","files.rs",E("if meta.owner_app != ctx.app_id || sha256_hex(&bytes) != meta.reference {"),"if sha256_hex(&bytes) != meta.reference {","files_owner_contract"),
 ("files-list-owner","files.rs",E(".filter(|meta| meta.owner_app == ctx.app_id)"),".filter(|_| true)","files_owner_contract"),
 ("process-env-clear","process.rs",E(".env_clear()"),"","process_env_contract"),
 ("process-cwd","process.rs",E("Some(path) if relative(path) => {"),"Some(path) => {","process_env_contract"),
 ("policy-actor","directory.rs",E("BusinessOsActor::new(Some(ctx.actor.id.clone()), &ctx.actor.role)"),'BusinessOsActor::new(Some(ctx.actor.id.clone()), "admin")',"directory_policy_contract"),
 ("command-scope","directory.rs",E("WHERE command_id=?1 AND module=?2"),"WHERE command_id=?1 AND ?2=?2","directory_policy_contract"),
 ("auth-assist-module","browser.rs",E(".is_some_and(|module| module != ctx.app_id)"),".is_some_and(|_| false)","auth_assist_contract"),
 # ---- CAP-3c (§16.46)
 ("core-app-binding",PEER,E("if bound_app != binding.app_id {"),"if false {","browser_core_entry_contract"),
 ("core-user-binding",PEER,E("if bound_user != binding.user_id {"),"if false {","browser_core_entry_contract"),
 ("core-policy",PEER,E("if !decision.allowed {"),"if false {","browser_core_entry_contract"),
 ("core-audit",PEER,E('core_browser_audit(root, binding, command_type, session_id, "allowed", "")'),"Ok(())","browser_core_entry_contract"),
 ("core-internal-only",PEER,E("if let Some(owner) = core_owner {"),'if let Some(owner) = core_owner.or_else(|| document.pointer("/client_context/actor/id").and_then(Value::as_str)) {',"browser_internal_only_contract"),
 ("mail-redact","mail.rs",E('|| !(k.contains("password")'),"|| !(false","mail_server_intake_contract"),
]
SELECTED=set(sys.argv[2].split(",")) if len(sys.argv)>2 and sys.argv[2] else None
HASH_ONLY="--restore-hash-only" in sys.argv[3:]
if SELECTED:
    known={proof[0] for proof in PROOFS}
    if not SELECTED<=known: raise RuntimeError("unknown mutation selector: "+",".join(sorted(SELECTED-known)))
    PROOFS=[proof for proof in PROOFS if proof[0] in SELECTED]
results=[]
canonical_exit=None
restored_exit=None
try:
    code,output=run("baseline","app_capabilities::cap3::tests")
    canonical_exit=code
    if code: print(output[-14000:],flush=True);raise RuntimeError("canonical baseline failed")
    print("BASELINE GREEN",flush=True)
    for label,filename,pattern,replacement,suffix in PROOFS:
        path=SOURCE/filename
        base=ORIGINAL[path].decode()
        matches=list(re.finditer(pattern,base))
        if len(matches)!=1:raise RuntimeError(label+": mutation match is not unique: "+str(len(matches)))
        line=base.count("\n",0,matches[0].start())+1
        mutated=re.sub(pattern,lambda _:replacement,base,count=1)
        path.write_text(mutated)
        qualified="app_capabilities::cap3::tests::app_capabilities_cap3_"+suffix
        try:
            code,output=run(label,qualified,True)
            red=code!=0 and qualified+" ... FAILED" in output and "could not compile" not in output
            result={"mutation":label,"file":str(path.relative_to(ROOT)),"line":line,
                    "before":matches[0].group(),"after":replacement,"test":qualified,"exit":code,"assertion_failure":red}
            results.append(result)
            print("MUTATION "+json.dumps(result,separators=(",",":")),flush=True)
            if not red:
                print(output[-14000:],flush=True)
        finally:path.write_bytes(ORIGINAL[path])
    if digest()!=original_digest:raise RuntimeError("source restoration hash mismatch")
    if HASH_ONLY:
        print("RESTORED HASHES MATCH the green canonical baseline; no additional restored run requested",flush=True)
    else:
        code,output=run("restored","app_capabilities::cap3::tests")
        restored_exit=code
        if code: print(output[-14000:],flush=True);raise RuntimeError("restored canonical checks failed")
        print("RESTORED GREEN; source hashes unchanged",flush=True)
    survivors=[r["mutation"] for r in results if not r["assertion_failure"]]
    if survivors: raise RuntimeError("mutations survived: "+",".join(survivors))
finally:
    for path,data in ORIGINAL.items():path.write_bytes(data)
    (OUT/"proofs.json").write_text(json.dumps({"proofs":results,"source_sha256":digest(),"restored":digest()==original_digest,"canonical_exit":canonical_exit,"restored_exit":restored_exit,"hash_only":HASH_ONLY},indent=2)+"\n")
    print("PROOFS_JSON "+json.dumps({"proofs":[[r["mutation"],r["line"],r["assertion_failure"]] for r in results],"restored":digest()==original_digest}),flush=True)
