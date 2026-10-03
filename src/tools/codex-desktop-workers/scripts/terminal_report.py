#!/usr/bin/env python3
"""Terminal PR learning evidence and offline report. Standard library only."""
import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import statistics
import subprocess
import uuid

REPOS = ("metric-space-ai/ctox", "metric-space-ai/workjet", "mkh-welsch/ctox-dev")
STOP = {174, 177, 181, 188, 189}
WEIGHTS = dict(correctness=.30, assignment_fulfillment=.25, evidence_quality=.20,
               closure_quality=.15, efficiency=.10)
RUBRIC = "unified-actor-v1"
BASE = Path.home() / ".codex/proxy-workers"

def command(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.PIPE, timeout=90)

def read(path):
    return command("greppy", "rg", "--no-heading", "--no-line-number", "^", str(path))

def load(path):
    return json.loads(read(path))

def save(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    stage = path.with_name(path.name + "." + uuid.uuid4().hex + ".writing")
    stage.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    stage.chmod(0o600)
    os.replace(stage, path)

def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()

def sha(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False).encode()).hexdigest()

def jobs(base):
    return [load(p) for p in sorted((base / "jobs").glob("*.json"))]

def terminal(pr):
    return pr.get("state") in ("MERGED", "CLOSED")

def stop(pr):
    return pr["repository"] == REPOS[0] and pr["number"] in STOP

def collect(base, repositories=REPOS):
    """Only request terminal states; snapshot history is content addressed."""
    existing = jobs(base)
    extra = {j["pr_url"] for j in existing if j.get("pr_url") and j.get("repository") not in repositories}
    gathered = []
    fields = """number title url state isDraft headRefName headRefOid createdAt closedAt mergedAt additions deletions changedFiles body
        mergeCommit {oid} author {login}
        files(first:100){totalCount pageInfo{hasNextPage} nodes{path additions deletions}}
        reviews(first:100){totalCount pageInfo{hasNextPage} nodes{body state submittedAt url commit{oid} author{login}}}
        comments(first:100){totalCount pageInfo{hasNextPage} nodes{body createdAt url author{login}}}
        commits(last:1){nodes{commit{oid committedDate statusCheckRollup{contexts(first:100){totalCount pageInfo{hasNextPage} nodes{
           ... on CheckRun{name conclusion status detailsUrl completedAt}
           ... on StatusContext{context state targetUrl}
        }}}}}}"""
    for repo in repositories:
        owner, name = repo.split("/")
        cursor = None
        total = None
        n = 0
        while True:
            after = ',after:' + json.dumps(cursor) if cursor else ''
            query = '{repository(owner:' + json.dumps(owner) + ',name:' + json.dumps(name) + '){pullRequests(first:25,states:[MERGED,CLOSED],orderBy:{field:CREATED_AT,direction:ASC}' + after + '){totalCount pageInfo{hasNextPage endCursor} nodes{' + fields + '}}}}'
            result = json.loads(command("gh", "api", "graphql", "-f", "query=" + query))
            if result.get("errors"):
                raise ValueError(result["errors"])
            page = result["data"]["repository"]["pullRequests"]
            total = page["totalCount"]
            for pr in page["nodes"]:
                if not terminal(pr):
                    raise ValueError("Nonterminal PR returned")
                pr.update(repository=repo, project="CTOX", snapshot_at=now())
                gathered.append(pr)
                n += 1
            if not page["pageInfo"]["hasNextPage"]:
                break
            cursor = page["pageInfo"]["endCursor"]
        if n != total:
            raise ValueError("Incomplete inventory " + repo)
        print(repo, n, "terminal PRs", flush=True)
    for url in sorted(extra):
        # Existing external registry cases only. Do not inventory unrelated repositories.
        pr = json.loads(command("gh", "pr", "view", url, "--json",
            "number,title,url,state,headRefName,headRefOid,createdAt,closedAt,mergedAt,additions,deletions,changedFiles,body,reviews,comments,files,statusCheckRollup"))
        if terminal(pr):
            pr.update(repository=url.split("github.com/")[1].split("/pull/")[0],
                      project="External registry", snapshot_at=now())
            gathered.append(pr)
    snapshot = dict(version=1, collected_at=now(), repositories=list(repositories), prs=gathered)
    destination = base / "terminal-evidence/snapshots" / (sha(snapshot) + ".json")
    save(destination, snapshot)
    save(base / "terminal-evidence/current.json", snapshot)
    print("Saved", len(gathered), "terminal PRs", flush=True)
    return snapshot

def weighted(result):
    if result is None:
        return None
    values = result.get("criteria")
    if values is None:
        if not result.get("unassessed_reason"):
            raise ValueError("Unassessed result requires a reason")
        return None
    if set(values) != set(WEIGHTS):
        raise ValueError("All five rubric criteria required; legacy totals cannot be decomposed")
    if any(v is not None and (type(v) not in (int, float) or not 0 <= v <= 10) for v in values.values()):
        raise ValueError("Criterion scores must be 0..10")
    if not result.get("evidence") or not result.get("rationale") or set(result.get("criterion_reasons",{})) != set(WEIGHTS):
        raise ValueError("Assessed result requires evidence and rationale")
    if any(v is None for v in values.values()):
        if not result.get("unassessed_reason"):
            raise ValueError("Missing criterion requires a specific unassessed reason")
        return None
    value = round(sum(values[k] * WEIGHTS[k] for k in WEIGHTS), 3)
    return min(value, 4) if result.get("essential_defect") else value

def record(base, entry):
    inventory = load(base / "terminal-evidence/current.json")
    prs = {p["url"]: p for p in inventory["prs"]}
    pr = prs.get(entry.get("pr_url"))
    if not pr or not terminal(pr) or stop(pr):
        raise ValueError("Assessments require an inventoried terminal, non-STOP PR")
    live = json.loads(command("gh", "pr", "view", entry["pr_url"], "--json", "state,headRefOid"))
    if not terminal(live) or live["headRefOid"] != pr["headRefOid"]:
        raise ValueError("PR is live or snapshot head changed; recollect terminal evidence")
    if entry.get("role") not in ("parent", "worker"):
        raise ValueError("Same schema requires parent or worker")
    if not entry.get("actor_id") or not entry.get("scope") or not entry.get("reviewer"):
        raise ValueError("Actor, obligations and independent reviewer required")
    if entry["reviewer"] == entry["actor_id"]:
        raise ValueError("Actor cannot assess itself")
    for stage in ("first", "corrected"):
        result = entry.get(stage)
        if result:
            result["weighted_total"] = weighted(result)
            if result["weighted_total"] is not None and not re.fullmatch("[a-f0-9]{40}", result.get("head") or ""):
                raise ValueError("Assessed result requires immutable source head")
    provenance = entry.get("provenance") or {}
    if entry.get("model") and not (provenance.get("turn_id") and provenance.get("evidence")):
        raise ValueError("Exact model requires exact authoring turn evidence")
    if provenance.get("model") != entry.get("model"):
        raise ValueError("Provenance model must match assessed author")
    key = sha([entry["pr_url"], entry["role"], entry["actor_id"], entry.get("package", "delivery")])
    entry.update(schema=RUBRIC, assessment_key=key, repository=pr["repository"],
                 recorded_at=now(), pr_head=pr["headRefOid"], disposition=pr["state"])
    folder = base / "assessments/unified-v1"
    folder.mkdir(parents=True, exist_ok=True)
    with (folder / "write.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        revisions = []
        for path in sorted(folder.glob(key + ".*.json")):
            revisions.append(load(path))
        entry["revision"] = len(revisions) + 1
        entry["supersedes"] = revisions[-1].get("record_id") if revisions else None
        entry["record_id"] = sha(entry)
        path = folder / (key + "." + str(entry["revision"]).zfill(4) + ".json")
        # Never overwrite history.
        with path.open("x") as output:
            json.dump(entry, output, ensure_ascii=False, indent=2)
            output.write("\n")
        path.chmod(0o600)
        if load(path) != entry:
            raise ValueError("Assessment readback differs")
    print("Recorded", entry["record_id"], entry["role"], entry["pr_url"])
    return entry

def assessments(base):
    latest = {}
    for path in sorted((base / "assessments/unified-v1").glob("*.json")):
        row = load(path)
        k = row["assessment_key"]
        if k not in latest or row["revision"] > latest[k]["revision"]:
            latest[k] = row
    return list(latest.values())

def legacy_rows(job, pr):
    """Expose actual earlier methodology. Phases remain detail, not samples."""
    result = []
    first = job.get("first_rating") or {}
    corrected = job.get("final_rating") or {}
    retrospective = job.get("retrospective_rating") or {}
    if first or corrected:
        result.append(dict(rubric="legacy-delivery-v1", role="worker",
            actor_id=job["thread_id"], registered_model=job.get("model"), model=None,
            first=first.get("score"), corrected=corrected.get("score"), note=corrected.get("note") or first.get("note"),
            rework=corrected.get("rework") or first.get("rework"), cause=corrected.get("cause") or first.get("cause"),
            first_head=first.get("pr_head"), corrected_head=corrected.get("pr_head"),
            history=job.get("rating_history", []), provenance_reason="Registry model is assignment metadata; authoring turn recovery pending"))
    if retrospective:
        result.append(dict(rubric="legacy-retrospective-v1", role="worker",
            actor_id=job["thread_id"], registered_model=job.get("model"), model=None,
            first=retrospective.get("first_score"), corrected=retrospective.get("final_score"),
            note=retrospective.get("note"), rework=retrospective.get("rework"), cause=retrospective.get("cause"),
            first_head=retrospective.get("first_head"), corrected_head=retrospective.get("final_head"),
            history=job.get("retrospective_history", []), provenance_reason="Registry model is assignment metadata; authoring turn recovery pending"))
    parent = job.get("orchestration_ratings", {})
    if not result and job.get("learning_review"):
        review = job["learning_review"]
        result.append(dict(rubric="legacy-qualitative-v1", role="worker", actor_id=job["thread_id"],
            registered_model=job.get("model"), model=None, first=None, corrected=None,
            note=review.get("evidence_note") or review.get("weaknesses"),
            rework="unknown", cause=review.get("failure_cause", "unknown"),
            history=job.get("review_history", [])))
    if parent:
        # Multiple phases are correlated observations of one package. No pooled total.
        result.append(dict(rubric="legacy-parent-phase-v1", role="parent", actor_id=job["parent_thread"],
                           model=None, registered_model=None, first=None, corrected=None,
                           phases=parent, history=job.get("orchestration_history", []),
                           note="Per-phase observations retained. No invented delivery score.", rework="unknown", cause="unknown"))
    for row in result:
        row.update(pr_url=pr["url"], repository=pr["repository"], state=pr["state"], project=pr["project"])
    return result

def cycle(pr):
    end = pr.get("mergedAt") or pr.get("closedAt")
    if not pr.get("createdAt") or not end:
        return None
    return round((dt.datetime.fromisoformat(end.replace("Z", "+00:00")) -
                  dt.datetime.fromisoformat(pr["createdAt"].replace("Z", "+00:00"))).total_seconds() / 3600, 2)

def stats(values):
    known = [v for v in values if v is not None]
    return dict(n=len(known), denominator=len(values),
                mean=round(statistics.mean(known), 3) if known else None,
                median=statistics.median(known) if known else None,
                distribution=[sum(lo <= v < hi for v in known) for lo, hi in [(0, 3), (3, 5), (5, 8), (8, 11)]])

def leaderboard_data(records):
    groups = {}
    seen = set()
    for row in records:
        key = (row["pr_url"], row["role"], row["actor_id"], row.get("rubric", row.get("schema")), row.get("package", "delivery"))
        if key in seen:
            continue
        seen.add(key)
        rubric = row.get("rubric", row.get("schema"))
        group = groups.setdefault((row["role"], row.get("model"), rubric), [])
        group.append(row)
    result = []
    for (role, model, rubric), rows in sorted(groups.items(), key=lambda x: str(x[0])):
        def score(row, stage):
            value = row.get(stage)
            return value.get("weighted_total") if isinstance(value, dict) else value
        first = [score(r, "first") for r in rows] if model else [None] * len(rows)
        corrected = [score(r, "corrected") for r in rows] if model else [None] * len(rows)
        rework = [r["rework"] for r in rows if r.get("rework") not in (None, "unknown")]
        result.append(dict(role=role, model=model, rubric=rubric, deliveries=len(rows),
                           first=stats(first), corrected=stats(corrected),
                           rework=dict(changed=sum(v != "none" for v in rework), denominator=len(rework))))
    return result


def build(base):
    snapshot = load(base / "terminal-evidence/current.json")
    prs = snapshot["prs"]
    mapping = {p["url"]: p for p in prs}
    records = assessments(base)
    legacy = []
    registry = jobs(base)
    for j in registry:
        if j.get("pr_url") in mapping:
            legacy.extend(legacy_rows(j, mapping[j["pr_url"]]))
    provenance_path = base / "terminal-evidence/provenance.json"
    recovered = load(provenance_path) if provenance_path.exists() else {}
    for row in legacy:
        prov = recovered.get(row["actor_id"], {})
        model_set = {r["model"] for r in prov.get("turns", []) if r.get("model")}
        # Require exact head-correlated turn; session-constant model alone is insufficient.
        exact = next((r for r in prov.get("bindings", []) if r.get("kind") == "source_action" and r.get("head") in
                     (row.get("first_head"), row.get("corrected_head")) and r.get("model")), None)
        if exact:
            row["model"] = exact["model"]
            row["provenance"] = exact
    database = Path.home() / ".codex/state_5.sqlite"
    if database.exists():
        connection = sqlite3.connect("file:" + str(database) + "?mode=ro", uri=True)
        for row in records + legacy:
            owner = connection.execute("select originator,source from threads where id=?", (row["actor_id"],)).fetchone()
            row["harness"] = owner[0] if owner and owner[0] else "Codex Desktop" if owner and owner[1] == "vscode" else None
        connection.close()
    urls = {r["pr_url"] for r in records if any((r.get(s) or {}).get("weighted_total") is not None for s in ("first", "corrected"))}
    summary = dict(terminal_prs=len(prs), core_terminal=sum(p["project"] == "CTOX" for p in prs),
                   merged=sum(p["state"] == "MERGED" for p in prs), closed=sum(p["state"] == "CLOSED" for p in prs),
                   unified_assessed_prs=len(urls), legacy_records=len(legacy), registry_jobs=len(registry),
                   registry_gpt61=sum(j.get("model") == "gpt-6.1-sol" for j in registry),
                   unified_first=stats([(r.get("first") or {}).get("weighted_total") for r in records]),
                   unified_corrected=stats([(r.get("corrected") or {}).get("weighted_total") for r in records]))
    for p in prs:
        p["cycle_hours"] = cycle(p)
        p["stop"] = stop(p)
        p["coverage_reason"] = ("Explicit STOP: historical inventory only" if p["stop"] else
            "Unified actor assessment present" if p["url"] in urls else
            "Legacy evidence present; five-criterion review pending" if any(r["pr_url"] == p["url"] for r in legacy) else
            "PR evidence collected; assignment/actor review not yet completed")
    data = dict(schema=RUBRIC, generated_at=now(), snapshot_at=snapshot["collected_at"],
                summary=summary, weights=WEIGHTS, prs=prs, assessments=records, legacy=legacy,
                leaderboards=leaderboard_data(records + legacy), scope_repositories=list(REPOS))
    save(base / "MODEL-EXPERIENCE.json", data)
    template = read(Path(__file__).with_name("terminal_report.html"))
    serialized = json.dumps(data, ensure_ascii=False).replace("<", "\\u003c").replace("\u2028", "\\u2028").replace("\u2029", "\\u2029")
    html = template.replace("__REPORT_DATA__", serialized)
    path = base / "MODEL-EXPERIENCE.html"
    stage = path.with_suffix(".html.writing")
    stage.write_text(html)
    stage.chmod(0o600)
    os.replace(stage, path)
    print(json.dumps(summary, indent=2))
    return data

def bootstrap(base):
    """Build first populated report from terminal lists while rich evidence collects."""
    prs = []
    for name, repo in zip(("ctox", "workjet", "ctox-dev"), REPOS):
        for p in load(base / (name + "-terminal-prs.json")):
            if terminal(p):
                p.update(repository=repo, project="CTOX", snapshot_at=now())
                prs.append(p)
    save(base / "terminal-evidence/current.json", dict(version=1, collected_at=now(), repositories=list(REPOS), prs=prs))
    return build(base)

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--base", type=Path, default=BASE)
    sub = p.add_subparsers(dest="operation", required=True)
    sub.add_parser("collect")
    sub.add_parser("build")
    sub.add_parser("bootstrap")
    a = sub.add_parser("assess")
    a.add_argument("json", type=Path)
    args = p.parse_args()
    if args.operation == "assess":
        record(args.base, load(args.json))
        build(args.base)
    else:
        globals()[args.operation](args.base)

if __name__ == "__main__":
    main()
