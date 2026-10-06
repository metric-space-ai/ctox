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

REPOS = ("metric-space-ai/ctox", "metric-space-ai/workjet", "mkh-welsch/ctox-dev",
         "metric-space-ai/greppy", "mkh-welsch/miltonticket-app")
PROJECTS = dict(zip(REPOS, ("CTOX", "Workjet", "ctox-dev", "Greppy", "miltonticket-app")))

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
    explicit = base / "terminal-evidence/explicit-terminal-prs.json"
    if explicit.exists():
        extra.update(load(explicit).get("urls", []))
    extra = {url for url in extra if url.split("github.com/")[-1].split("/pull/")[0] not in repositories}
    gathered = []
    fields = """number title url state isDraft baseRefName headRefName headRefOid createdAt closedAt mergedAt additions deletions changedFiles body
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
                pr.update(repository=repo, project=PROJECTS[repo], snapshot_at=now())
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
            "number,title,url,state,baseRefName,headRefName,headRefOid,createdAt,closedAt,mergedAt,additions,deletions,changedFiles,body,reviews,comments,files,statusCheckRollup"))
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

def validate_source_reference(result):
    """A reviewed working-tree patch is immutable evidence, not an authored commit."""
    snapshot = result.get("source_snapshot")
    if snapshot is None:
        if not re.fullmatch("[a-f0-9]{40}", result.get("head") or ""):
            raise ValueError("Assessed result requires immutable source head or patch snapshot")
        return
    if not isinstance(snapshot, dict) or snapshot.get("kind") != "uncommitted_patch":
        raise ValueError("Source snapshot must identify an uncommitted patch")
    if result.get("head") is not None:
        raise ValueError("Uncommitted patch must not claim an authored source head")
    if not re.fullmatch("[a-f0-9]{40}", snapshot.get("base_head") or ""):
        raise ValueError("Patch snapshot requires its immutable base commit")
    patch = snapshot.get("patch")
    if not isinstance(patch, str) or not patch.startswith("diff --git ") or not patch.endswith("\n"):
        raise ValueError("Patch snapshot requires complete retained Git diff text")
    if hashlib.sha256(patch.encode()).hexdigest() != snapshot.get("sha256"):
        raise ValueError("Patch snapshot content differs from its immutable hash")
    evidence = snapshot.get("evidence")
    if not isinstance(evidence, list) or not evidence or not all(isinstance(e, str) and e.strip() for e in evidence):
        raise ValueError("Patch snapshot requires historical delivery and source evidence")


def validate_merge_target(entry, pr):
    target = entry.get("merge_target")
    if target is None:
        return None
    if not isinstance(target, dict) or pr.get("state") != "MERGED":
        raise ValueError("Historical merge target requires a merged PR")
    if entry.get("role") != "parent" or (entry.get("parent_completion") or {}).get("weighted_total") is None:
        raise ValueError("Historical merge target belongs to the assessed closing parent")
    branch = target.get("branch")
    evidence = target.get("evidence")
    if not isinstance(branch, str) or not branch.strip():
        raise ValueError("Historical merge target requires its actual branch")
    if target.get("head") != pr.get("headRefOid") or target.get("merged_at") != pr.get("mergedAt"):
        raise ValueError("Historical merge target must match the terminal head and merge time")
    if not re.fullmatch(r"[0-9a-f]{40}", str(target.get("merge_commit", ""))):
        raise ValueError("Historical merge target requires the actual merge commit")
    if not isinstance(evidence, list) or not evidence or not all(isinstance(e, str) and e.strip() for e in evidence):
        raise ValueError("Historical merge target requires retained merge evidence")
    return target


def historical_merge_target(pr, records):
    targets = [validate_merge_target(row, pr) for row in records
               if row.get("pr_url") == pr["url"] and row.get("merge_target") is not None]
    if not targets:
        return None
    identities = {(t["branch"], t["head"], t["merged_at"], t["merge_commit"]) for t in targets}
    if len(identities) != 1:
        raise ValueError("Conflicting historical merge targets")
    return targets[0]


def terminal_event(pr):
    """A reopened/reclosed PR is a distinct delivery even at the same head."""
    return {key: pr.get(key) for key in ("state", "headRefOid", "closedAt", "mergedAt")}

def current_assessment(row, pr):
    if row.get("terminal_event") is not None:
        return row["terminal_event"] == terminal_event(pr)
    # Old immutable records can only remain current for an unchanged event.
    # A late retrospective recording date is not proof of a new closing action.
    if pr.get("previous_terminal_events"):
        return False
    if row.get("pr_head") != pr.get("headRefOid") or row.get("disposition") != pr.get("state"):
        return False
    try:
        recorded = dt.datetime.fromisoformat(row["recorded_at"].replace("Z", "+00:00"))
        closed = dt.datetime.fromisoformat(pr["closedAt"].replace("Z", "+00:00"))
        return recorded >= closed
    except (KeyError, TypeError, ValueError):
        return False

def record(base, entry):
    inventory = load(base / "terminal-evidence/current.json")
    prs = {p["url"]: p for p in inventory["prs"]}
    pr = prs.get(entry.get("pr_url"))
    if not pr or not terminal(pr) or stop(pr):
        raise ValueError("Assessments require an inventoried terminal, non-STOP PR")
    live = json.loads(command("gh", "pr", "view", entry["pr_url"], "--json",
                              "state,headRefOid,closedAt,mergedAt"))
    if not terminal(live) or terminal_event(live) != terminal_event(pr):
        raise ValueError("PR is live or terminal event changed; recollect terminal evidence")
    event = terminal_event(pr)
    if entry.get("terminal_event") is not None and entry["terminal_event"] != event:
        raise ValueError("Assessment belongs to a different terminal event")
    if pr.get("previous_terminal_events") and entry.get("terminal_event") is None:
        raise ValueError("Reclosed PR requires explicit current terminal-event binding")
    if entry.get("role") not in ("parent", "worker"):
        raise ValueError("Same schema requires parent or worker")
    if not entry.get("actor_id") or not entry.get("scope") or not entry.get("reviewer"):
        raise ValueError("Actor, obligations and independent reviewer required")
    if entry["reviewer"] == entry["actor_id"]:
        raise ValueError("Actor cannot assess itself")
    for stage in ("first", "corrected", "parent_completion"):
        result = entry.get(stage)
        if result:
            if stage == "parent_completion" and entry["role"] != "parent":
                raise ValueError("PR-completion assessment belongs to a parent")
            result["weighted_total"] = weighted(result)
            if result["weighted_total"] is not None:
                validate_source_reference(result)
            if result.get("model"):
                prov = result.get("provenance") or entry.get("provenance") or {}
                if prov.get("model") != result["model"] or not (prov.get("turn_id") and prov.get("evidence")):
                    raise ValueError("Stage model requires matching actual turn evidence")
    validate_merge_target(entry, pr)
    provenance = entry.get("provenance") or {}
    if entry.get("model") and not (provenance.get("turn_id") and provenance.get("evidence")):
        raise ValueError("Exact model requires exact authoring turn evidence")
    if provenance.get("model") != entry.get("model"):
        raise ValueError("Provenance model must match assessed author")
    key = sha([entry["pr_url"], entry["role"], entry["actor_id"], entry.get("package", "delivery")])
    entry.update(schema=RUBRIC, assessment_key=key, repository=pr["repository"],
                 recorded_at=now(), pr_head=pr["headRefOid"], disposition=pr["state"],
                 terminal_event=event)
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

def stage_model(row, stage):
    result = row.get(stage)
    if isinstance(result, dict):
        if "model" in result:
            return result["model"]
        if result.get("provenance", {}).get("model"):
            return result["provenance"]["model"]
    return row.get(stage + "_model", row.get("model"))


def rework_iterations(row, model=None):
    """Count proved corrective deliveries; never infer them from commits or reratings."""
    count = row.get("rework_iterations")
    if type(count) is not int or count < 0:
        return None
    if model is None or count == 0:
        return count
    models = {stage_model(row, s) for s in ("first", "corrected", "parent_completion")} - {None}
    if models == {model}:
        return count
    evidence = row.get("iteration_evidence")
    if isinstance(evidence, list):
        corrections = [e for e in evidence if isinstance(e, dict) and e.get("kind") == "correction"]
        if len(corrections) == count and all(e.get("model") for e in corrections):
            return sum(e["model"] == model for e in corrections)
    return None


def whole_pr_iterations(records, pr_url):
    """Count all proved corrections to a PR, independently of its closing model."""
    rows = [a for a in records if a["pr_url"] == pr_url and a.get("schema", a.get("rubric")) == RUBRIC]
    whole = [a["rework_iterations"] for a in rows if a.get("iteration_scope") == "pr" and rework_iterations(a) is not None]
    if whole and len(set(whole)) == 1:
        return whole[0]
    if not rows or any(rework_iterations(a) is None for a in rows) or not any(a.get("iteration_history_complete") is True for a in rows):
        return None
    heads = set()
    for a in rows:
        events = [e for e in a.get("iteration_evidence", []) if e.get("kind") == "correction"]
        if rework_iterations(a) == 0:
            continue
        if len(events) != rework_iterations(a) or any(not e.get("head") for e in events):
            return None
        heads.update(e["head"] for e in events)
    return len(heads)


def harness_label(value):
    if value in ("Codex Desktop", "Codex"):
        return "codex"
    if value in ("Claude Desktop", "claude-desktop"):
        return "claude"
    return "grok" if value in ("native Grok Build CLI", "Grok Build CLI") else value

def partition_deliveries(records):
    deliveries, attempts = [], []
    for row in records:
        if row.get("delivery_status") != "not_delivered":
            deliveries.append(row)
            continue
        if not row.get("no_delivery_evidence") or any(
                (row.get(stage) or {}).get("weighted_total") is not None
                for stage in ("first", "corrected", "parent_completion")):
            raise ValueError("Non-delivery requires evidence and cannot hide numeric assessments")
        attempts.append(row)
    return deliveries, attempts


def leaderboard_data(records):
    groups = {}
    seen = set()
    for row in records:
        key = (row["pr_url"], row["role"], row["actor_id"], row.get("rubric", row.get("schema")), row.get("package", "delivery"))
        if key in seen:
            continue
        seen.add(key)
        rubric = row.get("rubric", row.get("schema"))
        if row["role"] == "parent" and rubric == RUBRIC and (row.get("parent_completion") or {}).get("weighted_total") is None:
            continue
        stages = ("parent_completion",) if row["role"] == "parent" and rubric == RUBRIC else ("first", "corrected")
        for model in {stage_model(row, stage) for stage in stages}:
            groups.setdefault((row["role"], harness_label(row.get("harness")), model, rubric), []).append(row)
    result = []
    for (role, harness, model, rubric), rows in sorted(groups.items(), key=lambda x: str(x[0])):
        def score(row, stage):
            if model is None or stage_model(row, stage) != model:
                return None
            value = row.get(stage)
            return value.get("weighted_total") if isinstance(value, dict) else value
        independent_rows = rows
        if role == "worker" and rubric == RUBRIC:
            rows = [r for r in rows if r.get("first_end_scope_comparable") is not False and score(r,"first") is not None and score(r,"corrected") is not None]
        historical_parent = role == "parent" and rubric == RUBRIC
        first = [None if historical_parent else score(r, "first") for r in rows]
        corrected = [None if historical_parent else score(r, "corrected") for r in rows]
        completion = [score(r, "parent_completion") for r in rows]
        rework = [whole_pr_iterations(records, r["pr_url"]) if historical_parent else rework_iterations(r, model) for r in rows]
        known_rework = [v for v in rework if v is not None]
        rework_prs = len({r["pr_url"] for r in rows})
        iteration_total = sum(known_rework) if rows and len(known_rework) == len(rows) else None
        result.append(dict(role=role, harness=harness, model=model, rubric=rubric, deliveries=len(independent_rows), comparison_excluded=len(independent_rows)-len(rows),
                           prs=len({r["pr_url"] for r in rows if any(score(r,s) is not None for s in (("parent_completion",) if role == "parent" and rubric == RUBRIC else ("first", "corrected")))}),
                           score=stats(completion),
                           first=stats(first), corrected=stats(corrected),
                           rework=dict(iterations=iteration_total,
                                       mean=iteration_total / rework_prs if iteration_total is not None else None,
                                       observed_iterations=sum(known_rework) if known_rework else None,
                                       known=len(known_rework), unknown=len(rows)-len(known_rework))))
    return result



def parent_worker_pairs(records, prs):
    """Join source-proved parent/worker edges on the same PR, never by model coincidence."""
    parents = {}
    for row in records:
        if row["role"] == "parent" and (row.get("parent_completion") or {}).get("weighted_total") is not None:
            parents.setdefault((row["pr_url"], row["actor_id"]), []).append(row)
    pairs = []
    for worker in records:
        if worker["role"] != "worker" or not worker.get("parent_id"):
            continue
        for parent in parents.get((worker["pr_url"], worker["parent_id"]), []):
            pm = stage_model(parent, "parent_completion")
            fm, em = stage_model(worker, "first"), stage_model(worker, "corrected")
            first = (worker.get("first") or {}).get("weighted_total")
            end = (worker.get("corrected") or {}).get("weighted_total")
            if not pm or (first is None and end is None):
                continue
            pr = prs[worker["pr_url"]]
            ph, wh = harness_label(parent.get("harness")) or "—", harness_label(worker.get("harness")) or "—"
            worker_models = list(dict.fromkeys(m for m in (fm, em) if m))
            pairs.append(dict(pr_url=pr["url"], repository=pr["repository"], number=pr["number"],
                parent_id=parent["actor_id"], worker_id=worker["actor_id"],
                parent_record_id=parent["record_id"], worker_record_id=worker["record_id"],
                parent_model=pm, worker_first_model=fm, worker_end_model=em,
                parent_harness=ph, worker_harness=wh,
                parent_score=parent["parent_completion"]["weighted_total"], worker_first=first, worker_end=end,
                first_end_scope_comparable=worker.get("first_end_scope_comparable") is not False,
                combination=json.dumps([ph, pm, wh, worker_models], ensure_ascii=False)))
    return pairs

def build(base):
    snapshot = load(base / "terminal-evidence/current.json")
    prs = snapshot["prs"]
    for pr in prs:
        pr["project"] = PROJECTS.get(pr["repository"], "External registry")
    mapping = {p["url"]: p for p in prs}
    records, non_delivery_attempts = partition_deliveries(assessments(base))
    historical_assessments = [row for row in records + non_delivery_attempts
                              if row["pr_url"] in mapping and
                              not current_assessment(row, mapping[row["pr_url"]])]
    records = [row for row in records if row["pr_url"] in mapping and
               current_assessment(row, mapping[row["pr_url"]])]
    non_delivery_attempts = [row for row in non_delivery_attempts if row["pr_url"] in mapping and
                            current_assessment(row, mapping[row["pr_url"]])]
    legacy = []
    historical_legacy = []
    registry = jobs(base)
    for j in registry:
        if j.get("pr_url") in mapping:
            pr = mapping[j["pr_url"]]
            target = historical_legacy if pr.get("previous_terminal_events") else legacy
            target.extend(legacy_rows(j, pr))
    excluded = {(row["pr_url"], row["actor_id"]) for row in non_delivery_attempts}
    non_delivery_legacy = [row for row in legacy if (row["pr_url"], row["actor_id"]) in excluded]
    legacy = [row for row in legacy if (row["pr_url"], row["actor_id"]) not in excluded]
    provenance_path = base / "terminal-evidence/provenance.json"
    recovered = load(provenance_path) if provenance_path.exists() else {}
    for row in legacy:
        prov = recovered.get(row["actor_id"], {})
        model_set = {r["model"] for r in prov.get("turns", []) if r.get("model")}
        # Require exact head-correlated turn; session-constant model alone is insufficient.
        for stage in ("first", "corrected"):
            exact = next((r for r in prov.get("bindings", []) if r.get("kind") == "source_action"
                          and r.get("head") == row.get(stage + "_head") and r.get("model")), None)
            row[stage + "_model"] = exact["model"] if exact else None
            if exact:
                row.setdefault("provenance", {})[stage] = exact
        row["model"] = row.get("first_model") or row.get("corrected_model")
    database = Path.home() / ".codex/state_5.sqlite"
    workers = {j["thread_id"]: j for j in registry}
    for row in records + legacy:
        row["project"] = mapping[row["pr_url"]]["project"]
        row["parent_id"] = (row["actor_id"] if row["role"] == "parent" else
                            row.get("parent_id") or workers.get(row["actor_id"], {}).get("parent_thread") or
                            recovered.get(row["actor_id"], {}).get("native_parent_id"))
        row["worker_id"] = row["actor_id"] if row["role"] == "worker" else None
        row["reviewed_commit"] = row.get("pr_head") or mapping[row["pr_url"]]["headRefOid"]
    if database.exists():
        connection = sqlite3.connect("file:" + str(database) + "?mode=ro", uri=True)
        for row in records + legacy:
            owner = connection.execute("select originator,source from threads where id=?", (row["actor_id"],)).fetchone()
            row["harness"] = owner[0] if owner and owner[0] else "Codex Desktop" if owner and owner[1] == "vscode" else row.get("harness")
        connection.close()
    urls = {r["pr_url"] for r in records if any((r.get(s) or {}).get("weighted_total") is not None for s in ("first", "corrected", "parent_completion"))}
    summary = dict(terminal_prs=len(prs), core_terminal=sum(p["repository"] in REPOS for p in prs),
                   merged=sum(p["state"] == "MERGED" for p in prs), closed=sum(p["state"] == "CLOSED" for p in prs),
                   unified_assessed_prs=len(urls), legacy_records=len(legacy), registry_jobs=len(registry),
                   registry_gpt61=sum(j.get("model") == "gpt-6.1-sol" for j in registry),
                   unified_first=stats([(r.get("first") or {}).get("weighted_total") for r in records]),
                   unified_corrected=stats([(r.get("corrected") or {}).get("weighted_total") for r in records]),
                   parent_completion=stats([(r.get("parent_completion") or {}).get("weighted_total") for r in records if r["role"] == "parent"]))
    for p in prs:
        target = historical_merge_target(p, records)
        if target is not None:
            p["merge_target"] = target
        p["cycle_hours"] = cycle(p)
        p["stop"] = stop(p)
        p["coverage_reason"] = ("Explicit STOP: historical inventory only" if p["stop"] else
            "Unified actor assessment present" if p["url"] in urls else
            "Legacy evidence present; five-criterion review pending" if any(r["pr_url"] == p["url"] for r in legacy) else
            "PR evidence collected; assignment/actor review not yet completed")
    data = dict(schema=RUBRIC, generated_at=now(), snapshot_at=snapshot["collected_at"],
                summary=summary, weights=WEIGHTS, prs=prs, assessments=records, legacy=legacy,
                non_delivery_attempts=non_delivery_attempts, non_delivery_legacy=non_delivery_legacy,
                historical_assessments=historical_assessments, historical_legacy=historical_legacy,
                leaderboards=leaderboard_data(records + legacy), parent_worker_pairs=parent_worker_pairs(records, mapping), scope_repositories=list(REPOS))
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
    for repo in REPOS:
        name = repo.rsplit("/", 1)[1]
        for p in load(base / (name + "-terminal-prs.json")):
            if terminal(p):
                p.update(repository=repo, project=PROJECTS[repo], snapshot_at=now())
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
