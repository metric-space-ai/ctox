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
        row=dict(pr_url="pr",role="parent",actor_id="actor",schema=r.RUBRIC,model="gpt-6.1-sol",first={"weighted_total":4},corrected={"weighted_total":8},rework="substantial")
        output=r.leaderboard_data([row,copy.deepcopy(row)])
        self.assertEqual(output[0]["deliveries"],1)
        self.assertEqual(output[0]["corrected"]["mean"],8)
        row["model"]=None
        self.assertIsNone(r.leaderboard_data([row])[0]["corrected"]["mean"])
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
