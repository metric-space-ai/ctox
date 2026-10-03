"""Integrity tests: rubric, history, terminal-only boundaries and report statistics."""
import copy
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
                 rework_iterations=3,iteration_evidence=[{"kind":"correction","model":"actual-closing-model"}]*3)
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
        g=r.leaderboard_data([paired,firstonly,endonly])[0]
        self.assertEqual((g["prs"],g["first"]["n"],g["corrected"]["n"]),(1,1,1))
        self.assertEqual((g["first"]["mean"],g["corrected"]["mean"]),(4,8))
        self.assertEqual((g["deliveries"],g["comparison_excluded"]),(3,2))
        self.assertEqual(g["rework"]["iterations"],2)
    def test_pair_classification_ignores_missing_model_endpoint(self):
        parent=dict(pr_url="pr",role="parent",actor_id="p",record_id="p",
                    model="pm",harness="Codex Desktop",parent_completion={"weighted_total":8})
        worker=dict(pr_url="pr",role="worker",actor_id="w",parent_id="p",record_id="w",
                    model="wm",harness="Codex",first={"weighted_total":4},corrected={"weighted_total":7})
        other=copy.deepcopy(worker);other["actor_id"]="w2";other["first"]={"model":None,"weighted_total":None}
        pairs=r.parent_worker_pairs([parent,worker,other],{"pr":{"url":"pr","repository":"r","number":1}})
        self.assertEqual(pairs[0]["combination"],pairs[1]["combination"])
        self.assertEqual(pairs[0]["parent_harness"],"codex")
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

if __name__=="__main__":unittest.main()
