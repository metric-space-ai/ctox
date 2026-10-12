import unittest
import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location("analyzer", Path(__file__).with_name("workjet_guest_ab.py"))
analyzer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analyzer)
INSTANCE, distribution, summarize = analyzer.INSTANCE, analyzer.distribution, analyzer.summarize


def inputs():
    run = {"version": "0.0.89", "expectedVersion": "0.0.89", "source": "04ef8acae2cd63ca895081d5ddc03dea1b84a7ad",
           "arms": {"A": {"startUtc": "2026-10-11T10:00:00Z", "endUtc": "2026-10-11T10:10:00Z"},
                    "B": {"startUtc": "2026-10-11T10:11:00Z", "endUtc": "2026-10-11T10:21:00Z"}}}
    native = {"instanceId": INSTANCE, "readOnly": True, "commands": []}
    for arm in run["arms"].values():
        arm.update(version=run["version"], source=run["source"])
    return run, native


def span(sid, start_ms, cid=None, action="project.list", coalesced=False):
    return {"name": "CtoxGuestManager.requestProjectControl", "spanId": sid,
            "startTimeUnixNano": str(start_ms * 1000000), "endTimeUnixNano": str((start_ms + 30000) * 1000000),
            "durationMs": 30000, "attributes": {"instanceId": INSTANCE, "action": action,
            "commandId": cid, "outcome": "failed", "reason": "timeout", "request.coalesced": coalesced}}


class OfflineAnalysisTests(unittest.TestCase):
    def test_nearest_rank_and_missing_samples(self):
        self.assertEqual(distribution([100, 10, 20, 30])["p50Ms"], 20)
        self.assertEqual(distribution([100, 10, 20, 30])["p90Ms"], 100)
        self.assertIsNone(distribution([])["p50Ms"])

    def test_exact_late_claim_stays_in_arm_a_and_joined_spans_do_not_duplicate_command(self):
        run, native = inputs()
        start = 1791712800000  # fixture 2026-10-11T10:00:00Z
        native["commands"] = [{"commandId": "fixture-a", "action": "project.list",
                              "claimUtc": "2026-10-11T10:11:30Z"},
                             {"commandId": "fixture-b", "action": "project.list",
                              "claimUtc": "2026-10-11T10:11:10Z"}]
        rows = [span("a", start, "fixture-a"), span("a-joined", start + 1000, "fixture-a", coalesced=True),
                span("b", start + 660000, "fixture-b")]
        result = summarize(run, rows + [rows[0]], native)
        self.assertEqual(result["arms"]["A"]["desktopRequestCount"], 2)
        self.assertEqual(result["arms"]["A"]["startToClaimExact"]["n"], 1)
        self.assertEqual(result["arms"]["A"]["startToClaimExact"]["p50Ms"], 690000)
        self.assertEqual(result["arms"]["B"]["startToClaimExact"]["p50Ms"], 10000)
        self.assertFalse(result["installedAcceptance"])

    def test_no_shared_id_is_explicitly_unverified(self):
        run, native = inputs()
        native["commands"] = [{"commandId": "fixture-guess", "action": "project.list",
                              "claimUtc": "2026-10-11T10:00:12Z"}]
        result = summarize(run, [span("unknown", 1791712800000)], native)
        self.assertEqual(result["arms"]["A"]["startToClaimExact"]["n"], 0)
        self.assertEqual(result["arms"]["A"]["startToClaimOrderedUnverified"]["p50Ms"], 12000)
        self.assertEqual(result["arms"]["A"]["orderedAssociations"][0]["association"], "ordered_unverified")

    def test_wrong_version_and_conflicting_claims_are_rejected(self):
        run, native = inputs()
        run["version"] = "0.0.84"
        with self.assertRaises(ValueError):
            summarize(run, [], native)
        run["version"] = "0.0.89"
        native["commands"] = [{"commandId": "same", "action": "project.list", "claimUtc": utc}
                              for utc in ["2026-10-11T10:00:12Z", "2026-10-11T10:00:13Z"]]
        with self.assertRaises(ValueError):
            summarize(run, [], native)


class ReadbackTests(unittest.TestCase):
    def test_mixed_or_missing_arm_readback_is_rejected(self):
        for field, wrong in [("version", "0.0.85"), ("source", "f992afbddafccda1bb80eaf379f205b39aa47f9a")]:
            run, native = inputs()
            run["arms"]["B"][field] = wrong
            with self.assertRaises(ValueError):
                summarize(run, [], native)
        run, native = inputs()
        del run["arms"]["B"]["version"]
        with self.assertRaises(ValueError):
            summarize(run, [], native)

    def test_expected_version_is_explicit_and_missing_claims_stay_unknown(self):
        run, native = inputs()
        result = summarize(run, [], native)
        self.assertEqual(result["version"], "0.0.89")
        self.assertIsNone(result["arms"]["A"]["startToClaimExact"]["p50Ms"])
        del run["expectedVersion"]
        with self.assertRaises(ValueError):
            summarize(run, [], native)

    def test_short_or_overlapping_windows_are_rejected(self):
        run, native = inputs()
        run["arms"]["B"]["endUtc"] = "2026-10-11T10:20:00Z"
        with self.assertRaises(ValueError):
            summarize(run, [], native)
        run, native = inputs()
        run["arms"]["B"].update(startUtc="2026-10-11T10:09:00Z", endUtc="2026-10-11T10:19:00Z")
        with self.assertRaises(ValueError):
            summarize(run, [], native)


if __name__ == "__main__":
    unittest.main()
