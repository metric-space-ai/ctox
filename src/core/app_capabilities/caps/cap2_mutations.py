#!/usr/bin/env python3
"""CAP-2 contract mutation proofs. Run only through the GPU build lane."""
import datetime, hashlib, json, os, pathlib, re, signal, subprocess, sys, time
ROOT=pathlib.Path.cwd()
SOURCE=ROOT/"src/core/app_capabilities/caps"
OUT=pathlib.Path(os.environ["TMPDIR"])/("cap2-proofs-"+datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ"))
OUT.mkdir(parents=True,exist_ok=False)
# Leave time for durable source publication before this slice's hard stop.
DEADLINE=datetime.datetime.fromisoformat(sys.argv[1]).timestamp() if len(sys.argv)>1 else time.time()+2700
FILES=[SOURCE/name for name in ["office.rs","blob.rs","doc_extract.rs","mail.rs","speech.rs","cap2_tests.rs"]]+[SOURCE.parent/"mod.rs"]
ORIGINAL={path:path.read_bytes() for path in FILES}
def digest():
    return {str(path.relative_to(ROOT)):hashlib.sha256(path.read_bytes()).hexdigest() for path in FILES}
original_digest=digest()
(OUT/"source-sha256.json").write_text(json.dumps(original_digest,indent=2)+"\n")
print("CAP2_PROOF_DIR="+str(OUT),flush=True)
def run(label,selector,exact=False):
    remain=int(DEADLINE-time.time())
    if remain<90: raise RuntimeError("proof deadline reached; no next run started")
    args=["cargo","test","--locked","-p","ctox","--bin","ctox",selector,"--"]
    if exact: args+=["--exact"]
    args+=["--test-threads=1"]
    output=OUT/(label+".log")
    with output.open("w") as stream:
        process=subprocess.Popen(args,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
        print("RUN "+label+" owner=01a0ce3f-55e2 pid="+str(process.pid)+" output="+str(output)+" stop=completion-or-timeout",flush=True)
        try: code=process.wait(timeout=min(420,remain))
        except subprocess.TimeoutExpired:
            os.killpg(process.pid,signal.SIGTERM)
            try: process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid,signal.SIGKILL);process.wait()
            raise RuntimeError(label+" timed out")
    return code,output.read_text(errors="replace")
PROOFS=[
 ("blob-content","blob.rs",r'return Err\(CtoxError::new\(\s*ErrorCode::Conflict,\s*"blob content differs from CAS reference",?\s*\)\);',
  "return Ok(bytes);","file_boundary_contract"),
 ("office-base","office.rs",re.escape("if request.base_version != request.session.editor.sha256 {"),
  "if false {","office_version_and_formats_contract"),
 ("mail-terminal","mail.rs",re.escape('"terminal" => return claimed.result.ok_or_else(CtoxError::uncertain),'),
  '"terminal" => {},',"mail_delivery_once_contract"),
 ("mail-uncertain","mail.rs",r'_ => return Err\(CtoxError::uncertain\(\)\),',
  "_ => {},","mail_delivery_once_contract"),
 ("parquet-offset","doc_extract.rs",re.escape("request.skip,"),"0,","parquet_window_contract"),
 ("mail-query-scope","mail.rs",re.escape("(?2='[]' OR id IN (SELECT value FROM json_each(?2)))"),"(1=1)","mail_reads_contract"),
 ("wire-envelope","office.rs",re.escape('json!({"payload":args})'),"args","transport_contract"),
 ("dom-limit","doc_extract.rs",re.escape(".take(request.limit)"),".take(1000)","document_boundary_contract"),
]
results=[]
try:
    code,output=run("baseline","app_capabilities::cap2::tests")
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
        qualified="app_capabilities::cap2::tests::app_capabilities_cap2_"+suffix
        try:
            code,output=run(label,qualified,True)
            red=code!=0 and qualified+" ... FAILED" in output and "could not compile" not in output
            result={"mutation":label,"file":str(path.relative_to(ROOT)),"line":line,
                    "before":matches[0].group(),"after":replacement,"test":qualified,"exit":code,"assertion_failure":red}
            results.append(result)
            print("MUTATION "+json.dumps(result,separators=(",",":")),flush=True)
            if not red:
                print(output[-14000:],flush=True)
                raise RuntimeError(label+": mutation survived or was not an assertion failure")
        finally:path.write_bytes(ORIGINAL[path])
    if digest()!=original_digest:raise RuntimeError("source restoration hash mismatch")
    code,output=run("restored","app_capabilities::cap2::tests")
    if code: print(output[-14000:],flush=True);raise RuntimeError("restored canonical checks failed")
    print("RESTORED GREEN; source hashes unchanged",flush=True)
finally:
    for path,data in ORIGINAL.items():path.write_bytes(data)
    (OUT/"proofs.json").write_text(json.dumps({"proofs":results,"source_sha256":digest(),"restored":digest()==original_digest},indent=2)+"\n")
