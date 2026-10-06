#!/usr/bin/env python3
"""Import new terminal PR events without grading active work or inventing scores."""
import argparse
import datetime as dt
import fcntl
import json
from pathlib import Path

import terminal_report as report

START = "2026-10-05T22:00:00Z"  # 2026-10-06 00:00 Europe/Berlin
FIELDS = ("number,title,url,state,isDraft,baseRefName,headRefName,headRefOid,"
          "createdAt,closedAt,mergedAt,additions,deletions,changedFiles,body,"
          "mergeCommit,author,files,reviews,comments,commits,statusCheckRollup")


def instant(value):
    result = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if result.tzinfo is None:
        raise ValueError("An explicit UTC offset is required")
    return result.astimezone(dt.timezone.utc)


def recent_changes(repository, since):
    """REST updated order has no search's 1000-result cap; include reopen events."""
    page = 1
    seen = set()
    while True:
        endpoint = (f"repos/{repository}/pulls?state=all&sort=updated"
                    f"&direction=desc&per_page=100&page={page}")
        values = json.loads(report.command("gh", "api", endpoint))
        if not isinstance(values, list):
            raise ValueError("GitHub pull-request page is not an array")
        for value in values:
            if instant(value["updated_at"]) < since:
                continue
            number = value["number"]
            if number not in seen:
                seen.add(number)
                yield value
        if len(values) < 100 or instant(values[-1]["updated_at"]) < since:
            return
        page += 1


def connection(values, total=None):
    total = len(values) if total is None else total
    return dict(totalCount=total, pageInfo=dict(hasNextPage=total > len(values)),
                nodes=values)


def detail(repository, change):
    pr = json.loads(report.command("gh", "pr", "view", change["html_url"],
                                   "--json", FIELDS))
    if not report.terminal(pr):
        return None  # A concurrently reopened PR is not a terminal delivery.
    if pr["headRefOid"] != change["head"]["sha"]:
        raise ValueError("Terminal head changed during collection: " + pr["url"])
    if not pr.get("closedAt"):
        raise ValueError("Terminal PR has no closure timestamp: " + pr["url"])
    files = pr.pop("files")
    reviews = pr.pop("reviews")
    comments = pr.pop("comments")
    commits = pr.pop("commits")
    checks = pr.pop("statusCheckRollup") or []
    pr.update(repository=repository, project=report.PROJECTS[repository],
              snapshot_at=report.now(), files=connection(files, pr["changedFiles"]),
              reviews=connection(reviews), comments=connection(comments))
    pr["commits"] = dict(nodes=[])
    if commits:
        last = commits[-1]
        pr["commits"]["nodes"] = [dict(commit=dict(
            oid=last["oid"], committedDate=last["committedDate"],
            statusCheckRollup=dict(contexts=connection(checks))))]
    return pr


def reconcile(base, previous, changes, since, query_at):
    """Stage the whole five-repository delta before publishing any inventory."""
    mapping = {pr["url"]: dict(pr) for pr in previous["prs"]}
    if len(mapping) != len(previous["prs"]):
        raise ValueError("Duplicate PR URLs in current inventory")
    added, refreshed, reopened = [], [], []
    for repository in report.REPOS:
        for change in changes[repository]:
            url = change["html_url"]
            prior = mapping.get(url)
            if change["state"] == "open":
                if prior is not None:
                    reopened.append(url)
                    del mapping[url]
                continue
            closed = change.get("closed_at")
            if not closed or instant(closed) < since:
                continue
            state = "MERGED" if change.get("merged_at") else "CLOSED"
            if prior and (prior["state"], prior["headRefOid"], prior.get("closedAt"),
                          prior.get("mergedAt")) == (
                              state, change["head"]["sha"], closed,
                              change.get("merged_at")):
                continue
            pr = detail(repository, change)
            if pr is None:
                if prior is not None:
                    reopened.append(url)
                    del mapping[url]
                continue
            evidence = base / "terminal-evidence/source" / (
                "daily-" + repository.replace("/", "-") + "-" +
                str(pr["number"]) + "-" + report.sha(pr) + ".json")
            report.save(evidence, pr)
            pr["terminal_delta_evidence"] = str(evidence)
            mapping[url] = pr
            (refreshed if prior else added).append(url)
    if not all(report.terminal(pr) for pr in mapping.values()):
        raise ValueError("Active PR in terminal inventory")
    snapshot = dict(previous)
    history = report.retain_terminal_history(previous, list(mapping.values()))
    snapshot.update(repositories=list(report.REPOS), prs=list(mapping.values()),
                    delta_updated_at=query_at, terminal_event_history=history)
    return snapshot, dict(added=added, refreshed=refreshed, reopened=reopened)


def update(base, start=START):
    base = Path(base)
    directory = base / "terminal-evidence"
    directory.mkdir(parents=True, exist_ok=True)
    # One finite writer; no keepalive, polling service, or overlapping daily runs.
    with (directory / "daily-update.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        state_path = directory / "daily-update.json"
        state = report.load(state_path) if state_path.exists() else {}
        baseline = instant(state.get("start_at", start))
        cursor = instant(state.get("last_succeeded_at", start))
        since = max(baseline, cursor - dt.timedelta(days=1))
        query_at = report.now()  # Advance to query start, not end; catch concurrent merges.
        previous = report.load(directory / "current.json")
        changes = {repo: list(recent_changes(repo, since)) for repo in report.REPOS}
        snapshot, delta = reconcile(base, previous, changes, since, query_at)
        snapshot["daily_policy"] = dict(start_at=baseline.isoformat(),
            repositories=list(report.REPOS), timezone="Europe/Berlin",
            hour=19, minute=15, active_prs_assessed=False)
        if delta["added"] or delta["refreshed"] or delta["reopened"]:
            report.save(directory / "snapshots" / (report.sha(snapshot) + ".json"),
                        snapshot)
        report.save(directory / "current.json", snapshot)
        # No assessment API is called: recorded criteria/model provenance stay immutable.
        result = report.build(base)
        state.update(version=1, start_at=baseline.isoformat(),
                     last_succeeded_at=query_at, repositories=list(report.REPOS),
                     last_delta=delta, terminal_prs=len(result["prs"]),
                     assessed_prs=result["summary"]["unified_assessed_prs"])
        report.save(state_path, state)
        print(json.dumps(dict(at=query_at, **delta,
                              terminal_prs=len(result["prs"])), ensure_ascii=False))
        return state


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", type=Path, default=report.BASE)
    parser.add_argument("--start", default=START)
    args = parser.parse_args()
    update(args.base, args.start)


if __name__ == "__main__":
    main()

