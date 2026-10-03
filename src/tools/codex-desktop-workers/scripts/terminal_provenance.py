#!/usr/bin/env python3
"""Recover exact recorded turn provenance for terminal PR actors."""
import argparse
import json
from pathlib import Path
import re
import sqlite3
import subprocess
from terminal_report import BASE, REPOS, jobs, load, save, stop, now

def extract(thread, prs, registry_job=None):
    path=Path(thread["rollout_path"])
    if not path.exists():
        return dict(thread_id=thread["id"], reason="Recorded rollout path no longer exists", turns=[], bindings=[])
    relevant=[p for p in prs if not stop(p)]
    pattern=r'"type":\s*"turn_context"|"type":\s*"session_meta"|github.com/(metric-space-ai/(ctox|workjet|greppy|learnordie)|mkh-welsch/ctox-dev)/pull/|'
    heads={p["headRefOid"]:p["url"] for p in relevant}
    if registry_job:
        for r in registry_job.get("rating_history",[]):
            if r.get("pr_head"):heads[r["pr_head"]]=registry_job.get("pr_url")
        r=registry_job.get("retrospective_rating") or {}
        for s in ("first_head","final_head"):
            if r.get(s):heads[r[s]]=registry_job.get("pr_url")
    pattern+="|".join(re.escape(h[:7]) for h in heads)
    output=subprocess.run(["greppy","rg","--no-heading","--line-number",pattern,str(path)],text=True,capture_output=True,timeout=90)
    if output.returncode not in (0,1):raise ValueError(output.stderr)
    turns={}; events=[]; active_turn=None
    for line in output.stdout.splitlines():
        number,body=line.split(":",1)
        try:entry=json.loads(body)
        except json.JSONDecodeError:continue
        payload=entry.get("payload") or {}
        if entry.get("type")=="turn_context":
            tid=payload.get("turn_id")
            if tid and not tid.startswith("auto-compact"):
                value=dict(turn_id=tid,model=payload.get("model"),reasoning=payload.get("effort"),
                           timestamp=entry.get("timestamp"),evidence=str(path)+":"+number)
                if tid in turns and turns[tid]["model"]!=value["model"]:
                    value["model"]=None;value["conflict"]=True
                turns[tid]=value
                active_turn=tid
        if entry.get("type")=="response_item" and payload.get("type") in ("function_call_output","function_call","message"):
            if payload.get("type")=="message" and payload.get("role")!="assistant":continue
            meta=dict(payload.get("internal_chat_message_metadata_passthrough") or {})
            meta.setdefault("turn_id",active_turn)
            events.append((number,entry.get("timestamp"),payload,meta))
        if entry.get("type")=="event_msg" and payload.get("type")=="exec_command_end":
            events.append((number,entry.get("timestamp"),payload,{"turn_id":active_turn}))
    bindings=[]; claims=[]
    for number,stamp,payload,meta in events:
        tid=meta.get("turn_id") or payload.get("turn_id")
        ctx=turns.get(tid)
        if not ctx:continue
        # Ignore copied developer/user content; inspect only actor/tool execution.
        content=json.dumps(payload,ensure_ascii=False)
        urls=set(re.findall(r'https://github.com/[^\s"\\]+/pull/\d+',content))
        text=" ".join(c.get("text","") for c in payload.get("content",[]) if isinstance(c,dict))
        executed=meta.get("executed_tool_calls") or []
        cmds=" ".join(str((t.get("arguments") or {}).get("cmd","")) for t in executed if isinstance(t.get("arguments"),dict))
        if payload.get("type")=="exec_command_end":
            cmds+=" ".join(str(x.get("cmd","")) for x in payload.get("parsed_cmd",[]) if isinstance(x,dict))
        if payload.get("type")=="function_call":
            cmds+=str(payload.get("arguments",""))
        write=bool(re.search(r'git[^\n]{0,80}(commit|push)|greppy (patch|replace|write)|apply_patch',cmds))
        for head,url in heads.items():
            if head in content or head[:7] in content:
                bindings.append(dict(**ctx,head=head,pr_url=url,action_evidence=str(path)+":"+number,
                                     kind="source_action" if write else "head_reference"))
        if "gh pr create" in cmds:
            for url in urls:
                if any(p["url"]==url for p in relevant):
                    claims.append(dict(**ctx,pr_url=url,action_evidence=str(path)+":"+number,kind="pr_creation"))
        if text and urls:
            for url in urls:
                if any(p["url"]==url for p in relevant):
                    claims.append(dict(**ctx,pr_url=url,action_evidence=str(path)+":"+number,
                                       kind="assistant_report",excerpt=text[:1200]))
    return dict(thread_id=thread["id"],source=thread["source"],rollout_path=str(path),turns=list(turns.values()),
                bindings=bindings,claims=claims,recovered_at=now())

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--base",type=Path,default=BASE)
    parser.add_argument("--thread",action="append")
    args=parser.parse_args()
    prs=load(args.base/"terminal-evidence/current.json")["prs"]
    registry={j["thread_id"]:j for j in jobs(args.base) if any(p["url"]==j.get("pr_url") and not stop(p) for p in prs)}
    conn=sqlite3.connect("file:"+str(Path.home()/".codex/state_5.sqlite")+"?mode=ro",uri=True)
    conn.row_factory=sqlite3.Row
    threads=[dict(r) for r in conn.execute("select id,rollout_path,source,cwd,git_branch,created_at from threads")]
    branches={p.get("headRefName") for p in prs if not stop(p)}
    wanted=set(registry)|{j["parent_thread"] for j in registry.values()}
    for t in threads:
        if t.get("git_branch") in branches and t.get("git_branch")!="codex/shell-v2-layout-menu":
            wanted.add(t["id"])
    if args.thread:wanted=set(args.thread)
    dest=args.base/"terminal-evidence/provenance.json"
    recovered=load(dest) if dest.exists() else {}
    for t in threads:
        if t["id"] in wanted:
            recovered[t["id"]]=extract(t,prs,registry.get(t["id"]))
            save(dest,recovered)
            v=recovered[t["id"]]
            print(t["id"],len(v["turns"]),"turns",len(v["bindings"]),"head bindings",len(v.get("claims",[])),"PR claims",flush=True)
    # Record native leaf provenance only when a terminal-linked parent exists.
    parent_ids={k for k,v in recovered.items() if v.get("claims") or v.get("bindings")}
    for t in threads:
        try:
            source=json.loads(t["source"])
            subagent=source.get("subagent",{}) if isinstance(source,dict) else {}
            spawn=subagent.get("thread_spawn",{}) if isinstance(subagent,dict) else {}
            parent=spawn.get("parent_thread_id") if isinstance(spawn,dict) else None
        except (ValueError,TypeError):continue
        if parent in parent_ids and t["id"] not in recovered and not args.thread:
            recovered[t["id"]]=extract(t,prs)
            recovered[t["id"]]["native_parent_id"]=parent
            save(dest,recovered)
            v=recovered[t["id"]]
            print("native",t["id"],len(v["bindings"]),len(v.get("claims",[])),flush=True)
    print("Saved actor histories",len(recovered))

if __name__=="__main__":main()
