"""Integrity tests: rubric, history, terminal-only boundaries and report statistics."""
import copy
import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import terminal_report as r

class ReportTests(unittest.TestCase):
    def result(self, score=8):
        return dict(head="a"*40, criteria={k:score for k in r.WEIGHTS},
                    criterion_reasons={k:"Observed evidence" for k in r.WEIGHTS},
                    evidence=["immutable-review"], rationale="Checked source and outcome")
    def test_non_delivery_is_preserved_outside_delivery_statistics(self):
        delivery = dict(actor_id="real", pr_url="pr", first=self.result(4))
        delivery["first"]["weighted_total"] = 4
        failed = dict(actor_id="failed-proxy", pr_url="pr",
                      delivery_status="not_delivered", no_delivery_evidence=["503 before inference"],
                      first=dict(weighted_total=None), corrected=None)
        active, attempts = r.partition_deliveries([delivery, failed])
        self.assertEqual(active, [delivery])
        self.assertEqual(attempts, [failed])
        self.assertEqual(failed["actor_id"], "failed-proxy")
        hidden = copy.deepcopy(failed)
        hidden["first"] = dict(weighted_total=3)
        with self.assertRaises(ValueError):
            r.partition_deliveries([hidden])
        unsupported = copy.deepcopy(failed)
        unsupported.pop("no_delivery_evidence")
        with self.assertRaises(ValueError):
            r.partition_deliveries([unsupported])
    def test_native_harness_alias_keeps_raw_provenance(self):
        raw = "native Grok Build CLI"
        row = dict(pr_url="pr", role="worker", actor_id="native", schema=r.RUBRIC,
                   model="grok-4.7-build", harness=raw, first={"weighted_total":3.1},
                   corrected={"weighted_total":4}, rework_iterations=1)
        group = r.leaderboard_data([row])[0]
        self.assertEqual(group["harness"], "grok")
        self.assertEqual(row["harness"], raw)
        self.assertEqual((group["first"]["mean"], group["corrected"]["mean"]), (3.1,4))
    def test_native_claude_harness_alias_preserves_client_evidence(self):
        row = dict(pr_url="pr", role="parent", actor_id="native", schema=r.RUBRIC,
                   model="claude-fable-5-1", harness="claude-desktop",
                   parent_completion={"weighted_total":7.65}, rework_iterations=1,
                   iteration_scope="pr")
        group = r.leaderboard_data([row])[0]
        self.assertEqual(group["harness"], "claude")
        self.assertEqual(group["score"]["mean"], 7.65)
        self.assertEqual(row["harness"], "claude-desktop")
        self.assertEqual(r.harness_label("unknown-native-client"), "unknown-native-client")
    def test_claude_code_alias_combines_pr_means_without_rewriting_evidence(self):
        first = dict(pr_url="one", role="parent", actor_id="a", schema=r.RUBRIC,
                     model="claude-opus-5-5", harness="Claude Code",
                     parent_completion={"weighted_total":4}, rework_iterations=2,
                     iteration_scope="pr")
        second = copy.deepcopy(first)
        second.update(pr_url="two", actor_id="b", harness="claude",
                      parent_completion={"weighted_total":8}, rework_iterations=0)
        original = copy.deepcopy([first, second])
        groups = r.leaderboard_data([first, second, copy.deepcopy(first)])
        self.assertEqual(len(groups), 1)
        self.assertEqual((groups[0]["harness"], groups[0]["prs"]), ("claude", 2))
        self.assertEqual(groups[0]["score"]["mean"], 6)
        self.assertEqual(groups[0]["rework"]["mean"], 1)
        self.assertEqual([first, second], original)

    def test_codex_exec_joins_codex_harness_without_rewriting_native_client(self):
        row = dict(pr_url="cli", role="parent", actor_id="cli", schema=r.RUBRIC,
                   model="gpt-6.1-sol", harness="codex_exec",
                   parent_completion={"weighted_total":4}, rework_iterations=2,
                   iteration_scope="pr")
        desktop = copy.deepcopy(row)
        desktop.update(pr_url="desktop", actor_id="desktop", harness="Codex Desktop",
                       parent_completion={"weighted_total":8}, rework_iterations=0)
        original = copy.deepcopy([row, desktop])
        group = r.leaderboard_data([row, desktop])[0]
        self.assertEqual(len(r.leaderboard_data([row, desktop])), 1)
        self.assertEqual((group["harness"], group["prs"], group["score"]["mean"], group["rework"]["mean"]),
                         ("codex", 2, 6, 1))
        self.assertEqual([row, desktop], original)

    def merge_fixture(self):


        pr = dict(url="pr", state="MERGED", headRefOid="a"*40,
                  mergedAt="2026-09-13T05:13:49Z", baseRefName="main")
        target = dict(branch="codex/devops-unification", head="a"*40,
                      merged_at=pr["mergedAt"], merge_commit="b"*40,
                      evidence=["actual initial base, normal merge and ancestry receipts"])
        parent = dict(pr_url="pr", role="parent",
                      parent_completion=dict(weighted_total=7.75), merge_target=target)
        return pr, parent

    def test_historical_merge_target_preserves_current_github_base(self):
        pr, parent = self.merge_fixture()
        target = r.historical_merge_target(pr, [parent])
        self.assertEqual(target["branch"], "codex/devops-unification")
        self.assertEqual(pr["baseRefName"], "main")
        self.assertIsNone(r.historical_merge_target(pr, []))

    def test_historical_merge_target_rejects_unbound_or_unproved_claim(self):
        pr, parent = self.merge_fixture()
        for key, value in [("head", "c"*40), ("merged_at", "wrong"), ("branch", ""),
                           ("merge_commit", "not-a-commit"), ("evidence", [])]:
            bad = copy.deepcopy(parent)
            bad["merge_target"][key] = value
            with self.assertRaises(ValueError):
                r.validate_merge_target(bad, pr)
        for changed in [dict(parent, role="worker"), dict(parent, parent_completion=None)]:
            with self.assertRaises(ValueError):
                r.validate_merge_target(changed, pr)
        with self.assertRaises(ValueError):
            r.validate_merge_target(parent, dict(pr, state="CLOSED"))

    def test_conflicting_historical_merge_targets_are_not_silently_ranked(self):
        pr, parent = self.merge_fixture()
        other = copy.deepcopy(parent)
        other["merge_target"]["branch"] = "another-branch"
        with self.assertRaises(ValueError):
            r.historical_merge_target(pr, [parent, other])

    def test_weights_and_essential_cap(self):
        self.assertEqual(r.weighted(self.result()),8)
        result=self.result(9);result["essential_defect"]=True
        self.assertEqual(r.weighted(result),4)
    def test_unknown_is_not_zero(self):
        self.assertIsNone(r.weighted(dict(criteria=None,unassessed_reason="No original handoff")))
        result=self.result();result["criteria"]["closure_quality"]=None
        result["unassessed_reason"]="Assigned installed acceptance still lacks proof"
        self.assertIsNone(r.weighted(result))
        summary=r.stats([None,0,8])
        self.assertEqual((summary["n"],summary["denominator"],summary["mean"],summary["median"]),(2,3,4,4))
    def test_no_legacy_component_invention(self):
        with self.assertRaises(ValueError):r.weighted(dict(criteria={"total":8}))
        with self.assertRaises(ValueError):r.weighted(dict(criteria=None))
    def test_model_means_exclude_unknown_and_deduplicate(self):
        row=dict(pr_url="pr",role="worker",actor_id="actor",schema=r.RUBRIC,model="gpt-6.1-sol",first={"weighted_total":4},corrected={"weighted_total":8},rework="substantial")
        output=r.leaderboard_data([row,copy.deepcopy(row)])
        self.assertEqual(output[0]["deliveries"],1)
        self.assertEqual(output[0]["corrected"]["mean"],8)
        row["model"]=None
        self.assertIsNone(r.leaderboard_data([row])[0]["corrected"]["mean"])
    def test_first_and_corrected_models_remain_separate(self):
        row=dict(pr_url="pr",role="worker",actor_id="actor",schema=r.RUBRIC,model="gpt-6-astra",
                 first={"model":"gpt-6-astra","weighted_total":4},
                 corrected={"model":"gpt-6.1-sol","weighted_total":8},rework="bounded")
        groups={g["model"]:g for g in r.leaderboard_data([row,copy.deepcopy(row)])}
        self.assertEqual(r.stage_model(row,"first"),"gpt-6-astra")
        self.assertIsNone(groups["gpt-6-astra"]["first"]["mean"])
        self.assertIsNone(groups["gpt-6-astra"]["corrected"]["mean"])
        self.assertIsNone(groups["gpt-6.1-sol"]["first"]["mean"])
        self.assertEqual(r.stage_model(row,"corrected"),"gpt-6.1-sol")
        self.assertIsNone(groups["gpt-6.1-sol"]["corrected"]["mean"])
        self.assertEqual(groups["gpt-6.1-sol"]["deliveries"],1)
    def test_parent_completion_replaces_historical_parent_stages(self):
        row=dict(pr_url="pr",role="parent",actor_id="p",schema=r.RUBRIC,model="old",
                 first={"weighted_total":4},corrected={"weighted_total":4},
                 parent_completion={"weighted_total":7.8,"model":"actual-closing-model"},
                 rework_iterations=3,iteration_scope="pr",iteration_evidence=[{"kind":"correction","model":"actual-closing-model"}]*3)
        groups=r.leaderboard_data([row])
        self.assertEqual(len(groups),1)
        self.assertEqual(groups[0]["model"],"actual-closing-model")
        self.assertEqual(groups[0]["score"]["mean"],7.8)
        self.assertEqual(groups[0]["prs"],1)
        self.assertEqual(groups[0]["rework"]["iterations"],3)
        self.assertEqual(row["corrected"]["weighted_total"],4)
    def test_pairs_require_parent_edge_same_pr_and_real_scores(self):
        parent=dict(pr_url="pr",role="parent",actor_id="p",record_id="parent-record",
                    model="parent-model",harness="Codex",parent_completion={"weighted_total":8})
        worker=dict(pr_url="pr",role="worker",actor_id="w",parent_id="p",record_id="worker-record",
                    model="worker-model",harness="Codex",first={"weighted_total":4},corrected={"weighted_total":7})
        prs={"pr":{"url":"pr","repository":"repo","number":1}}
        pairs=r.parent_worker_pairs([parent,worker],prs)
        self.assertEqual(len(pairs),1)
        self.assertEqual((pairs[0]["parent_score"],pairs[0]["worker_first"],pairs[0]["worker_end"]),(8,4,7))
        wrong=copy.deepcopy(worker);wrong["parent_id"]="someone-else"
        self.assertEqual(r.parent_worker_pairs([parent,wrong],prs),[])
        missing=copy.deepcopy(worker);missing["corrected"]=None
        self.assertIsNone(r.parent_worker_pairs([parent,missing],prs)[0]["worker_end"])
        expanded=copy.deepcopy(worker);expanded["first_end_scope_comparable"]=False
        pair=r.parent_worker_pairs([parent,expanded],prs)[0]
        self.assertFalse(pair["first_end_scope_comparable"])
        self.assertEqual((pair["worker_first"],pair["worker_end"]),(4,7))
    def test_iterations_are_evidenced_absolute_counts(self):
        self.assertIsNone(r.rework_iterations({"rework":"substantial"}))
        self.assertEqual(r.rework_iterations({"rework_iterations":0}),0)
        row={"rework_iterations":3,"model":"a","first":{"model":"a"},"corrected":{"model":"b"},
             "iteration_evidence":[{"kind":"correction","model":"a"},{"kind":"correction","model":"b"},{"kind":"correction","model":"b"}]}
        self.assertEqual(r.rework_iterations(row,"b"),2)
        row["iteration_evidence"].pop()
        self.assertIsNone(r.rework_iterations(row,"b"))
    def test_worker_comparison_uses_identical_pr_cohort(self):
        paired=dict(pr_url="paired",role="worker",actor_id="a",schema=r.RUBRIC,model="m",
                    first={"weighted_total":4},corrected={"weighted_total":8},rework_iterations=2)
        firstonly=copy.deepcopy(paired);firstonly.update(pr_url="first-only",actor_id="b",corrected=None)
        endonly=copy.deepcopy(paired);endonly.update(pr_url="end-only",actor_id="c",first=None)
        expanded=copy.deepcopy(paired);expanded.update(pr_url="expanded-scope",actor_id="d",
            first_end_scope_comparable=False,rework_iterations=0,first={"weighted_total":7.3},corrected={"weighted_total":7.8})
        g=r.leaderboard_data([paired,firstonly,endonly,expanded])[0]
        self.assertEqual((g["prs"],g["first"]["n"],g["corrected"]["n"]),(1,1,1))
        self.assertEqual((g["first"]["mean"],g["corrected"]["mean"]),(4,8))
        self.assertEqual((g["deliveries"],g["comparison_excluded"]),(4,3))
        self.assertEqual(g["rework"]["iterations"],2)
    def test_rework_leaderboard_averages_counts_per_compared_pr(self):
        a=dict(pr_url="one",role="worker",actor_id="a",schema=r.RUBRIC,model="m",
               first={"weighted_total":4},corrected={"weighted_total":8},rework_iterations=8)
        b=copy.deepcopy(a);b.update(pr_url="two",actor_id="b",rework_iterations=3)
        g=r.leaderboard_data([a,b])[0]
        self.assertEqual(g["prs"],2)
        self.assertEqual(g["rework"]["iterations"],11)
        self.assertEqual(g["rework"]["mean"],5.5)
        self.assertEqual((a["rework_iterations"],b["rework_iterations"]),(8,3))
        b["rework_iterations"]=None
        self.assertIsNone(r.leaderboard_data([a,b])[0]["rework"]["mean"])
        a["rework_iterations"]=0;b["rework_iterations"]=0
        self.assertEqual(r.leaderboard_data([a,b])[0]["rework"]["mean"],0)
    def test_parent_rework_covers_entire_pr_not_closing_model(self):
        a=dict(pr_url="one",role="parent",actor_id="p",schema=r.RUBRIC,model="parent-model",
               parent_completion={"weighted_total":8},rework_iterations=9,iteration_scope="pr",
               iteration_evidence=[dict(kind="correction",head=str(i),model="worker-model" if i<5 else "parent-model") for i in range(9)])
        b=copy.deepcopy(a);b.update(pr_url="two",rework_iterations=0,iteration_evidence=[])
        g=r.leaderboard_data([a,b])[0]
        self.assertEqual((g["prs"],g["rework"]["iterations"],g["rework"]["mean"]),(2,9,4.5))
        a["iteration_scope"]="actor";a["iteration_history_complete"]=False
        self.assertIsNone(r.leaderboard_data([a,b])[0]["rework"]["mean"])
        a["iteration_history_complete"]=True
        self.assertEqual(r.whole_pr_iterations([a,b],"one"),9)
    def test_pair_classification_ignores_missing_model_endpoint(self):
        parent=dict(pr_url="pr",role="parent",actor_id="p",record_id="p",
                    model="pm",harness="Codex Desktop",parent_completion={"weighted_total":8})
        worker=dict(pr_url="pr",role="worker",actor_id="w",parent_id="p",record_id="w",
                    model="wm",harness="Codex",first={"weighted_total":4},corrected={"weighted_total":7})
        other=copy.deepcopy(worker);other["actor_id"]="w2";other["first"]={"model":None,"weighted_total":None}
        pairs=r.parent_worker_pairs([parent,worker,other],{"pr":{"url":"pr","repository":"r","number":1}})
        self.assertEqual(pairs[0]["combination"],pairs[1]["combination"])
        self.assertEqual(pairs[0]["parent_harness"],"codex")
    def patch_result(self):
        result = self.result(4)
        result["head"] = None
        text = "diff --git a/repair.rs b/repair.rs\n--- a/repair.rs\n+++ b/repair.rs\n@@ -1 +1 @@\n-old\n+new\n"
        result["source_snapshot"] = dict(kind="uncommitted_patch", base_head="a"*40,
            patch=text, sha256=hashlib.sha256(text.encode()).hexdigest(),
            evidence=["original review-ready handoff", "retained full original patch"])
        return result

    def test_patch_snapshot_rejects_mutation_missing_base_and_fake_head(self):
        good = self.patch_result()
        r.validate_source_reference(good)
        for field, value in (("base_head", None), ("sha256", "f"*64),
                             ("evidence", []), ("kind", "authored_commit"),
                             ("patch", "diff --git incomplete")):
            bad = copy.deepcopy(good)
            bad["source_snapshot"][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                r.validate_source_reference(bad)
        bad = copy.deepcopy(good)
        bad["head"] = "a"*40
        with self.assertRaises(ValueError):
            r.validate_source_reference(bad)
        with self.assertRaises(ValueError):
            r.validate_source_reference({"head": None})

    def test_uncommitted_first_delivery_is_retained_without_invented_commit(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            pr = dict(url="https://github.com/metric-space-ai/ctox/pull/213",
                      repository=r.REPOS[0], number=213, state="MERGED", headRefOid="b"*40)
            r.save(base/"terminal-evidence/current.json", {"prs":[pr]})
            data = dict(pr_url=pr["url"], role="worker", actor_id="worker",
                        reviewer="independent", scope="Own original patch",
                        model=None, provenance={}, first=self.patch_result())
            real_command = r.command
            with patch.object(r, "command", side_effect=lambda *a:
                json.dumps({"state":"MERGED", "headRefOid":"b"*40}) if a[0]=="gh" else real_command(*a)):
                first = r.record(base, copy.deepcopy(data))
                data["corrected"] = self.result(8)
                second = r.record(base, copy.deepcopy(data))
            self.assertIsNone(second["first"]["head"])
            self.assertEqual(second["first"], first["first"])
            self.assertEqual(second["first"]["source_snapshot"], data["first"]["source_snapshot"])
            self.assertEqual(second["corrected"]["head"], "a"*40)
            self.assertEqual(r.assessments(base)[0]["record_id"], second["record_id"])

    def test_immutable_revisions_and_reject_self_score_live_stop(self):
        with tempfile.TemporaryDirectory() as temp:
            base=Path(temp)
            pr=dict(url="https://github.com/metric-space-ai/ctox/pull/213",repository=r.REPOS[0],number=213,state="MERGED",headRefOid="b"*40)
            r.save(base/"terminal-evidence/current.json",{"prs":[pr]})
            data=dict(pr_url=pr["url"],role="parent",actor_id="actor",reviewer="independent",scope="Own source, review and closure",model=None,provenance={},
                      first=self.result(4),corrected=self.result(8))
            real_command=r.command
            with patch.object(r,"command",side_effect=lambda *a: json.dumps({"state":"MERGED","headRefOid":"b"*40}) if a[0]=="gh" else real_command(*a)):
                one=r.record(base,copy.deepcopy(data));two=r.record(base,copy.deepcopy(data))
                self.assertEqual(two["supersedes"],one["record_id"])
                self.assertEqual(len(list((base/"assessments/unified-v1").glob("*.json"))),2)
                self.assertEqual(r.assessments(base)[0]["revision"],2)
                bad=copy.deepcopy(data);bad["reviewer"]="actor"
                with self.assertRaises(ValueError):r.record(base,bad)
            with patch.object(r,"command",side_effect=lambda *a: json.dumps({"state":"OPEN","headRefOid":"b"*40}) if a[0]=="gh" else real_command(*a)):
                with self.assertRaises(ValueError):r.record(base,copy.deepcopy(data))
            pr["number"]=174
            r.save(base/"terminal-evidence/current.json",{"prs":[pr]})
            with self.assertRaises(ValueError):r.record(base,copy.deepcopy(data))
    def test_rendered_json_matches_canonical_and_leaderboards(self):
        if not r.BASE.joinpath("MODEL-EXPERIENCE.html").exists():self.skipTest("No operator report")
        data=r.load(r.BASE/"MODEL-EXPERIENCE.json")
        html=r.read(r.BASE/"MODEL-EXPERIENCE.html")
        raw=html.split('<script id="report-data" type="application/json">',1)[1].split("</script>",1)[0]
        self.assertEqual(json.loads(raw),data)
        self.assertEqual(data["leaderboards"],r.leaderboard_data(data["assessments"]+data["legacy"]))
        self.assertTrue(all(r.terminal(p) for p in data["prs"]))
        self.assertEqual(data["summary"]["terminal_prs"],len(data["prs"]))
        self.assertIn('id="prev"',html)
        self.assertIn('id="next"',html)

class InventoryStorageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name) / "report"
        self.root = Path(self.temp.name) / "raw"
        self.remote = {}
        self.receipts = {}
        self.addCleanup(patch.stopall)
        patch.object(r.evidence_storage, "ROOT", self.root).start()
        patch.object(r, "save_raw", side_effect=self.remote_write).start()
        patch.object(r.evidence_storage, "locator", side_effect=lambda p:self.receipts.get(Path(p))).start()
        patch.object(r, "read", side_effect=lambda p:self.remote[Path(p)] if Path(p) in self.remote else Path(p).read_text()).start()
        self.snapshot = dict(version=1, collected_at="2026-10-10T17:00:00Z",
                             prs=[dict(url="terminal-pr", state="MERGED", body="evidence")])
        self.current = self.base / "terminal-evidence/current.json"

    def remote_write(self, path, value):
        text = json.dumps(value, ensure_ascii=False, indent=2) + "\n"
        self.remote[Path(path)] = text
        self.receipts[Path(path)] = dict(file=str(Path(path).relative_to(self.root)),
            bytes=len(text.encode()), sha256=hashlib.sha256(text.encode()).hexdigest())

    def test_large_inventory_is_remote_and_build_input_is_reproducible(self):
        self.snapshot["prs"][0]["body"] = "x" * 20_000_001
        reference = r.save_inventory(self.base, self.snapshot)
        self.assertGreater(reference["bytes"], 20_000_000)
        self.assertLess(self.current.stat().st_size, 1000)
        self.assertFalse(self.root.exists())
        self.assertEqual(r.load(self.current), self.snapshot)
        archived = list((self.base / "terminal-evidence/snapshots").glob("*.json"))
        self.assertEqual(len(archived), 1)
        self.assertEqual(r.load(archived[0]), self.snapshot)

    def test_remote_failure_preserves_existing_local_inventory(self):
        r.save(self.current, self.snapshot)
        original = self.current.read_bytes()
        with patch.object(r, "save_raw", side_effect=RuntimeError("transport failure")):
            with self.assertRaisesRegex(RuntimeError, "transport failure"):
                r.save_inventory(self.base, self.snapshot)
        self.assertEqual(self.current.read_bytes(), original)

    def test_legacy_raw_is_sha_verified_before_reference_replacement(self):
        r.save(self.current, self.snapshot)
        original = self.current.read_bytes()
        r.save_inventory(self.base, self.snapshot)
        backups = [p for p in self.remote if p.name.startswith("legacy-")]
        self.assertEqual(len(backups), 1)
        self.assertEqual(self.remote[backups[0]].encode(), original)
        self.assertEqual(r.load(self.current), self.snapshot)

    def test_noncanonical_legacy_bytes_are_kept_on_checksum_mismatch(self):
        self.current.parent.mkdir(parents=True)
        self.current.write_text(json.dumps(self.snapshot))
        original = self.current.read_bytes()
        with self.assertRaisesRegex(ValueError, "Legacy inventory bytes"):
            r.save_inventory(self.base, self.snapshot)
        self.assertEqual(self.current.read_bytes(), original)

    def test_reference_rejects_tampering_traversal_and_active_prs(self):
        reference = r.save_inventory(self.base, self.snapshot)
        tampered = dict(reference, sha256="0"*64)
        r.save(self.current, tampered)
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            r.load(self.current)
        r.save(self.current, dict(reference, file="../other.json"))
        with self.assertRaisesRegex(ValueError, "Unsafe"):
            r.load(self.current)
        active = copy.deepcopy(self.snapshot)
        active["prs"][0]["state"] = "OPEN"
        reference = r.save_inventory(self.base, active)
        with self.assertRaisesRegex(ValueError, "metadata mismatch"):
            r.load(self.current)


if __name__=="__main__":unittest.main()
