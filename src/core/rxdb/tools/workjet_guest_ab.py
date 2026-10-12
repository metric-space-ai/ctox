#!/usr/bin/env python3
"""Offline metadata-only A/B analysis. Never controls Workjet or opens its stores."""
import argparse
import collections
import datetime
import json
import math
import re
from pathlib import Path

INSTANCE = "managed:322084e5-8239-48d7-b3c5-c5178fbe5822"


def utc_ms(value):
    parsed = datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("UTC timestamp must include timezone")
    return parsed.timestamp() * 1000


def distribution(values):
    ordered = sorted(values)
    def rank(p):
        return ordered[max(0, math.ceil(p * len(ordered)) - 1)] if ordered else None
    return {"n": len(ordered), "p50Ms": rank(.5), "p90Ms": rank(.9),
            "minMs": ordered[0] if ordered else None, "maxMs": ordered[-1] if ordered else None,
            "method": "nearest rank"}


def summarize(run, desktop, native):
    expected_version = run.get("expectedVersion")
    if not isinstance(expected_version, str) or not re.fullmatch(r"\d+\.\d+\.\d+", expected_version):
        raise ValueError("Missing explicit expected installed Workjet version")
    if run.get("version") != expected_version:
        raise ValueError("Installed version differs from expected Workjet version")
    if not isinstance(run.get("source"), str) or not re.fullmatch(r"[0-9a-f]{40}", run["source"]):
        raise ValueError("Missing actual installed source SHA")
    if native.get("instanceId") != INSTANCE or native.get("readOnly") is not True:
        raise ValueError("Native claims must be read-only metadata for Welsch")
    windows = {}
    for arm in ("A", "B"):
        observed = run["arms"][arm]
        if observed.get("version") != expected_version or observed.get("source") != run["source"]:
            raise ValueError("Each arm needs matching installed version and source readback")
        start = utc_ms(run["arms"][arm]["startUtc"])
        end = utc_ms(run["arms"][arm]["endUtc"])
        if abs(end - start - 600000) > 1000:
            raise ValueError("Each arm must measure 600 seconds")
        windows[arm] = (start, end)
    if windows["B"][0] < windows["A"][1]:
        raise ValueError("Arm windows cannot overlap")
    spans = []
    seen = set()
    for item in desktop:
        if item.get("name") != "CtoxGuestManager.requestProjectControl":
            continue
        attrs = item.get("attributes", {})
        if attrs.get("instanceId") != INSTANCE or not item.get("spanId"):
            continue
        if item["spanId"] in seen:
            continue
        seen.add(item["spanId"])
        spans.append({"spanId": item["spanId"], "action": attrs.get("action"),
                      "start": int(item["startTimeUnixNano"]) / 1000000,
                      "end": int(item["endTimeUnixNano"]) / 1000000,
                      "duration": float(item["durationMs"]),
                      "commandId": attrs.get("guestCommandId") or attrs.get("commandId"),
                      "outcome": attrs.get("outcome"), "reason": attrs.get("reason"),
                      "phase": attrs.get("phase"), "coalesced": attrs.get("request.coalesced") is True})
    claims = {}
    for row in native.get("commands", []):
        cid = row["commandId"]
        normalized = {"commandId": cid, "action": row["action"], "claim": utc_ms(row["claimUtc"])}
        if cid in claims and claims[cid] != normalized:
            raise ValueError("Conflicting native claim metadata for one command")
        claims[cid] = normalized
    output = {"version": run["version"], "expectedVersion": expected_version, "source": run["source"], "instanceId": INSTANCE,
              "nativeSource": native.get("source"), "clockAlignmentVerified": native.get("clockAlignmentVerified", False),
              "installedAcceptance": False, "arms": {}}
    all_exact_ids = {row["commandId"] for row in spans if row["commandId"]}
    for arm, (start, end) in windows.items():
        rows = sorted([r for r in spans if start <= r["start"] < end], key=lambda r: r["start"])
        exact = []
        negative = []
        unmatched = 0
        seen_command_ids = set()
        for row in rows:
            if row["commandId"]:
                if row["commandId"] in seen_command_ids:
                    continue
                seen_command_ids.add(row["commandId"])
            claim = claims.get(row["commandId"])
            if not claim or claim["action"] != row["action"]:
                unmatched += 1
                continue
            delay = claim["claim"] - row["start"]
            if delay < 0:
                negative.append(row["spanId"])
                continue
            exact.append({"spanId": row["spanId"], "commandId": claim["commandId"],
                          "delayMs": delay, "claimedAfterDesktopDeadline": claim["claim"] > row["end"]})
        temporal_rows = [r for r in rows if r["action"] == "project.list" and not r["commandId"] and not r["coalesced"]]
        temporal_claims = sorted([c for c in claims.values() if c["action"] == "project.list"
                                 and c["commandId"] not in all_exact_ids
                                 and start <= c["claim"] < end + 180000], key=lambda c: c["claim"])
        pairs = []
        for row, claim in zip(temporal_rows, temporal_claims):
            delay = claim["claim"] - row["start"]
            if delay >= 0:
                pairs.append({"spanId": row["spanId"], "candidateCommandId": claim["commandId"],
                              "delayMs": delay, "association": "ordered_unverified"})
        per_action = {}
        for action in sorted({r["action"] for r in rows if r["action"]}):
            selected = [r for r in rows if r["action"] == action]
            per_action[action] = {"count": len(selected), "duration": distribution([r["duration"] for r in selected]),
                                  "outcomes": dict(collections.Counter(r["reason"] or r["outcome"] for r in selected))}
        output["arms"][arm] = {"startUtc": run["arms"][arm]["startUtc"], "endUtc": run["arms"][arm]["endUtc"],
                              "desktopRequestCount": len(rows), "coalescedRequestCount": sum(r["coalesced"] for r in rows),
                              "uniqueKnownCommandIdCount": len(seen_command_ids),
                              "guestExecutionRequestCount": sum(not r["coalesced"] for r in rows),
                              "nativeClaimsInsideWindow": sum(start <= c["claim"] < end for c in claims.values()),
                              "byAction": per_action, "startToClaimExact": distribution([r["delayMs"] for r in exact]),
                              "exactAssociations": exact, "unmatchedDesktopRequests": unmatched,
                              "invalidNegativeAssociations": negative,
                              "startToClaimOrderedUnverified": distribution([r["delayMs"] for r in pairs]),
                              "orderedAssociations": pairs, "temporalDesktopCount": len(temporal_rows),
                              "temporalNativeCandidateCount": len(temporal_claims),
                              "temporalCountMismatch": len(temporal_rows) != len(temporal_claims),
                              "temporalLimit": "Not causal identification. Late A commands can enter B; shared candidate claims can occur in both windows. Do not compare as exact."}
    shared = {r["candidateCommandId"] for r in output["arms"]["A"]["orderedAssociations"]} & {
        r["candidateCommandId"] for r in output["arms"]["B"]["orderedAssociations"]}
    output["orderedCandidatesUsedByBothArms"] = sorted(shared)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", required=True)
    parser.add_argument("--desktop", required=True)
    parser.add_argument("--claims", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    run = json.loads(Path(args.run).read_text())
    native = json.loads(Path(args.claims).read_text())
    with Path(args.desktop).open() as source:
        desktop = [json.loads(line) for line in source if line.strip()]
    result = summarize(run, desktop, native)
    target = Path(args.out)
    if target.exists():
        raise FileExistsError("Output exists; preserve earlier evidence")
    with target.open("x") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    target.chmod(0o600)
    print(json.dumps({"output": str(target), "arms": {a: {
        "count": r["desktopRequestCount"], "exact": r["startToClaimExact"],
        "ordered_unverified": r["startToClaimOrderedUnverified"]} for a, r in result["arms"].items()}}))


if __name__ == "__main__":
    main()
