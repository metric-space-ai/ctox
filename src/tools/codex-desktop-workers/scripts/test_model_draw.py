import copy,datetime as dt,tempfile,unittest
from pathlib import Path
import model_draw as draw
NOW=dt.datetime(2026,10,13,tzinfo=dt.timezone.utc)
class DrawTests(unittest.TestCase):
    def pool(self):
        rows=[dict(model_id=m,provider="openai",harness="codex",home_harness="codex",family="gpt",
                   route_id=m,model_list_evidence="verified-test-export",
                   enabled=True,coding=True,available=True)
              for m in ("gpt-6.1-sol","gpt-6-astra")]
        return dict(schema=draw.SCHEMA,status="ready",source="workjet-registered-models",
                    evidence="test-export",refreshed_at=NOW.isoformat(),models=rows)
    def test_accounts_do_not_inflate_a_models_draw_probability(self):
        pool=self.pool()
        duplicate=copy.deepcopy(pool["models"][0]);duplicate["route_id"]="second-account"
        pool["models"].append(duplicate)
        with tempfile.TemporaryDirectory() as folder:
            result=draw.draw(pool,"parent","assignment","brief",folder,NOW,pick=lambda keys:keys[0])
            self.assertEqual([p["probability"] for p in result["probabilities"]],[.5,.5])
            self.assertIsNone(result["actual_authoring_model"])
    def test_assignment_cannot_reroll_or_change_its_brief(self):
        with tempfile.TemporaryDirectory() as folder:
            first=draw.draw(self.pool(),"parent","assignment","brief",folder,NOW,pick=lambda keys:keys[0])
            second=draw.draw({},"parent","assignment","brief",folder,NOW,pick=lambda keys:keys[-1])
            self.assertEqual(first,second)
            with self.assertRaises(ValueError):
                draw.draw(self.pool(),"parent","assignment","changed",folder,NOW)
    def test_unverified_stale_or_wrong_home_harness_is_rejected(self):
        for change in ("status","time","harness"):
            pool=self.pool()
            if change=="status":pool["status"]="awaiting-export"
            if change=="time":pool["refreshed_at"]=(NOW-dt.timedelta(hours=1)).isoformat()
            if change=="harness":pool["models"][0]["harness"]="claude-code"
            with self.assertRaises(ValueError):draw.eligible(pool,NOW)
if __name__=="__main__":unittest.main()
