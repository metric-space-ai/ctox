#!/usr/bin/env python3
"""Immutable uniform Worker draws from a fresh, verified Workjet model export."""
import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess

BASE = Path.home() / ".codex/proxy-workers"
SCHEMA = "workjet-worker-model-pool-v1"

def read(path):
    return json.loads(subprocess.check_output(
        ["greppy", "rg", "--no-heading", "--no-line-number", "^", str(path)], text=True))

def eligible(pool, now):
    if pool.get("schema") != SCHEMA or pool.get("status") != "ready":
        raise ValueError("Verified Workjet model/harness export is not ready; no default or guessed model")
    if pool.get("source") != "workjet-registered-models" or not pool.get("evidence"):
        raise ValueError("Pool requires official Workjet catalog evidence, not Worker profiles")
    refreshed = dt.datetime.fromisoformat(pool["refreshed_at"].replace("Z", "+00:00"))
    age = (now-refreshed).total_seconds()
    if not 0 <= age <= 900:
        raise ValueError("Refresh the official Workjet catalog before delegating; pool is stale")
    models = {}
    for row in pool.get("models", []):
        if not (row.get("enabled") and row.get("coding") and row.get("available")):
            continue
        if not all(row.get(k) for k in ("model_id", "provider", "harness", "home_harness", "route_id", "model_list_evidence")):
            raise ValueError("Eligible model is missing a proved ID, home harness, route or live-list evidence")
        if row["harness"] != row["home_harness"]:
            raise ValueError("Configured route does not match the verified home harness")
        if row.get("family") == "gpt" and row["harness"] != "codex":
            raise ValueError("GPT Worker requires its Codex home harness")
        if row.get("family") == "claude" and row["harness"] != "claude-code":
            raise ValueError("Claude Worker requires its Claude Code home harness")
        key = (row["provider"], row["model_id"])
        models.setdefault(key, []).append(row)
    if not models:
        raise ValueError("No eligible registered coding model; retain the assignment without a fallback")
    return models

def draw(pool, parent_id, assignment_id, brief_hash, directory, now=None, pick=None):
    if not all((parent_id, assignment_id, brief_hash)):
        raise ValueError("Parent, stable assignment ID and brief hash are required")
    now = now or dt.datetime.now(dt.timezone.utc)
    identity = hashlib.sha256((parent_id+"\n"+assignment_id).encode()).hexdigest()
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    directory.chmod(0o700)
    with (directory/(identity+".lock")).open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        target = directory/(identity+".json")
        if target.exists():
            choice = read(target)
            if choice["brief_sha256"] != brief_hash:
                raise ValueError("Assignment already drawn with a different brief; do not reroll")
            return choice
        candidates = eligible(pool, now)
        keys = sorted(candidates)
        chosen = (pick or secrets.choice)(keys)
        route = sorted(candidates[chosen], key=lambda r:r["route_id"])[0]
        record = dict(schema="worker-model-draw-v1", parent_id=parent_id,
                      assignment_id=assignment_id, brief_sha256=brief_hash,
                      selected_at=now.isoformat(), mode="uniform",
                      selection_id=identity, model_id=route["model_id"],
                      provider=route["provider"], harness=route["harness"],
                      route_id=route["route_id"], catalog_evidence=pool["evidence"],
                      pool_sha256=hashlib.sha256(json.dumps(pool, sort_keys=True).encode()).hexdigest(),
                      probabilities=[dict(provider=p, model_id=m, probability=1/len(keys)) for p,m in keys],
                      execution={k:route[k] for k in ("app_server_provider", "supported_reasoning", "route_evidence") if k in route},
                      actual_authoring_model=None, worker_id=None, pr_url=None)
        stage = target.with_suffix(".writing")
        with stage.open("w") as out:
            json.dump(record, out, ensure_ascii=False, indent=2)
            out.write("\n")
            out.flush()
            os.fsync(out.fileno())
        stage.chmod(0o600)
        os.replace(stage, target)
        return record

def selection_for_create(args, base, jobs):
    """Draw before a new task; retain existing packages and native home routes."""
    base, jobs = Path(base), Path(jobs)
    assignment = args.issue
    for path in sorted(jobs.glob("*.json")):
        job = read(path)
        if job.get("parent_thread") == args.parent_thread and job.get("issue_url") == assignment:
            raise ValueError("Assignment already owns task " + job["thread_id"] +
                             "; recover the same task, never create another Worker")
    identity = hashlib.sha256((args.parent_thread+"\n"+assignment).encode()).hexdigest()
    legacy = base/"selections"/(identity+".json")
    existing_draw = base/"model-draws"/(identity+".json")
    if legacy.exists() and not existing_draw.exists():
        raise ValueError("Legacy assignment already has a selection; preserve it and recover its existing task, do not redraw")
    brief = subprocess.check_output(["greppy", "rg", "--no-heading", "--no-line-number", "^", str(args.prompt_file)], text=True)
    if not brief.strip():
        raise ValueError("Assignment must not be empty")
    # Hash file bytes rather than text output: CRLF and final-newline changes matter.
    with Path(args.prompt_file).open("rb") as source:
        brief_hash = hashlib.sha256(source.read()).hexdigest()
    pool = read(base/"MODEL-POOL.json") if not existing_draw.exists() else {}
    choice = draw(pool, args.parent_thread, assignment, brief_hash, base/"model-draws")
    if getattr(args, "model", None) and args.model != choice["model_id"]:
        raise ValueError("Assignment drew " + choice["model_id"] + "; --model cannot override it or request a redraw")
    if choice["harness"] != "codex" or choice["provider"] != "openai":
        raise ValueError("Assignment drew " + choice["model_id"] + " on " + choice["harness"] +
                         "; this starter supports native OpenAI Codex only. Retain this draw for its registered home starter; no Codex proxy fallback")
    execution = choice.get("execution", {})
    if execution.get("app_server_provider") != "openai" or not execution.get("route_evidence"):
        raise ValueError("Draw lacks a verified native Codex route; retain it without a fallback")
    if args.reasoning not in execution.get("supported_reasoning", []):
        raise ValueError("Reasoning effort is not supported by the drawn route; retain the draw and use its advertised effort")
    args.model = choice["model_id"]
    return dict(choice, id=choice["selection_id"], model=choice["model_id"],
                issue=assignment, parent_thread=args.parent_thread,
                task_type=getattr(args, "task_type", "unspecified"),
                difficulty=getattr(args, "difficulty", "standard"), reasoning=args.reasoning)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--parent", required=True)
    parser.add_argument("--assignment", required=True)
    parser.add_argument("--brief-sha256", required=True)
    args=parser.parse_args()
    record=draw(read(BASE/"MODEL-POOL.json"), args.parent, args.assignment,
                args.brief_sha256, BASE/"model-draws")
    print(json.dumps(record, ensure_ascii=False))

if __name__=="__main__":
    main()
