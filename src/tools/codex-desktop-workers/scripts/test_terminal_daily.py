"""Terminal delta integrity: pagination, reopening, history and durable cursors."""
import datetime as dt
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import terminal_daily as daily
import terminal_report as report


class DailyTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        # Test fixtures are local data; production source reads still go through Greppy.
        self.reader = patch.object(report, "read", lambda p: Path(p).read_text())
        self.reader.start()
        self.addCleanup(self.reader.stop)
        # Storage transport is covered separately; these tests isolate terminal reconciliation.
        self.inventory_writer = patch.object(report, "save_inventory",
            lambda base, snapshot: report.save(base / "terminal-evidence/current.json", snapshot))
        self.inventory_writer.start()
        self.addCleanup(self.inventory_writer.stop)
        self.storage_root = patch.object(report.evidence_storage, "ROOT", self.base / "raw")
        self.storage_root.start()
        self.addCleanup(self.storage_root.stop)
        self.raw_writer = patch.object(report, "save_raw", lambda path, value: report.save(path, value))
        self.raw_writer.start()
        self.addCleanup(self.raw_writer.stop)
        self.since = daily.instant(daily.START)

    def change(self, number, state="closed", closed=daily.START, head="a" * 40):
        return dict(number=number, html_url=f"https://github.com/metric-space-ai/ctox/pull/{number}",
                    state=state, updated_at="2026-10-06T12:00:00Z", closed_at=closed,
                    merged_at=closed if state == "closed" else None, head=dict(sha=head))

    def pr(self, change):
        return dict(number=change["number"], url=change["html_url"], state="MERGED",
                    repository=report.REPOS[0], headRefOid=change["head"]["sha"],
                    closedAt=change["closed_at"], mergedAt=change["merged_at"])

    def changes(self, *values):
        return {repo: list(values) if repo == report.REPOS[0] else []
                for repo in report.REPOS}

    def test_pagination_is_not_limited_to_search_thousand_results(self):
        pages = [[self.change(n) for n in range(i * 100, (i + 1) * 100)]
                 for i in range(11)]
        pages.append([self.change(1100)])
        with patch.object(report, "command", side_effect=[json.dumps(p) for p in pages]) as command:
            result = list(daily.recent_changes(report.REPOS[0], self.since))
        self.assertEqual(len(result), 1101)
        self.assertEqual(command.call_count, 12)
        self.assertIn("page=12", command.call_args.args[-1])

    def test_updated_order_carries_reopens_and_stops_before_old_pages(self):
        page = [self.change(1, state="open")] + [dict(self.change(n),
                updated_at="2026-10-01T00:00:00Z") for n in range(2, 101)]
        with patch.object(report, "command", return_value=json.dumps(page)) as command:
            result = list(daily.recent_changes(report.REPOS[0], self.since))
        self.assertEqual([p["number"] for p in result], [1])
        command.assert_called_once()

    def test_reconcile_keeps_history_deduplicates_and_removes_reopened(self):
        original = dict(self.pr(self.change(1)), historical_model_note="immutable")
        reopened = self.pr(self.change(2))
        newly_closed = self.change(3)
        before = dict(prs=[original, reopened], collected_at="old-complete-snapshot")
        with patch.object(daily, "detail", return_value=self.pr(newly_closed)) as detail:
            after, delta = daily.reconcile(self.base, before,
                self.changes(self.change(1), self.change(2, state="open"), newly_closed),
                self.since, "2026-10-06T13:00:00Z")
        self.assertEqual(after["prs"][0], original)
        self.assertEqual(before["prs"], [original, reopened])
        self.assertEqual(len(after["prs"]), 2)
        self.assertEqual(delta["added"], [newly_closed["html_url"]])
        self.assertEqual(delta["reopened"], [reopened["url"]])
        self.assertEqual(delta["refreshed"], [])
        self.assertEqual(after["collected_at"], "old-complete-snapshot")
        detail.assert_called_once()

    def test_old_terminal_events_are_not_backfilled_into_new_repository_scope(self):
        change = self.change(7, closed="2026-10-01T00:00:00Z")
        with patch.object(daily, "detail") as detail:
            after, delta = daily.reconcile(self.base, dict(prs=[]),
                                          self.changes(change), self.since, "now")
        self.assertEqual(after["prs"], [])
        self.assertEqual(delta["added"], [])
        detail.assert_not_called()

    def test_detail_rejects_changed_head_and_ignores_concurrent_reopen(self):
        change = self.change(1)
        with patch.object(report, "command", return_value=json.dumps(dict(
                state="MERGED", url=change["html_url"], headRefOid="b" * 40))):
            with self.assertRaisesRegex(ValueError, "head changed"):
                daily.detail(report.REPOS[0], change)
        with patch.object(report, "command", return_value='{"state":"OPEN"}'):
            self.assertIsNone(daily.detail(report.REPOS[0], change))

    def test_cursor_catches_missed_days_and_is_not_advanced_after_failure(self):
        directory = self.base / "terminal-evidence"
        report.save(directory / "current.json", dict(prs=[], collected_at="historic"))
        prior = dict(start_at=daily.START, last_succeeded_at="2026-10-07T17:15:00Z")
        report.save(directory / "daily-update.json", prior)
        with patch.object(daily, "recent_changes", return_value=[]) as query, \
             patch.object(report, "now", return_value="2026-10-11T17:15:00Z"), \
             patch.object(report, "build", side_effect=ValueError("failed render")):
            with self.assertRaisesRegex(ValueError, "failed render"):
                daily.update(self.base)
        self.assertEqual(report.load(directory / "daily-update.json"), prior)
        self.assertEqual(query.call_count, 5)
        self.assertTrue(all(call.args[1] == daily.instant("2026-10-06T17:15:00Z")
                            for call in query.call_args_list))

    def test_success_records_watermark_without_assessment_or_fake_scores(self):
        directory = self.base / "terminal-evidence"
        report.save(directory / "current.json", dict(prs=[], collected_at="historic"))
        with patch.object(daily, "recent_changes", return_value=[]), \
             patch.object(report, "now", return_value="2026-10-06T17:15:00Z"), \
             patch.object(report, "build", return_value=dict(
                 prs=[], summary=dict(unified_assessed_prs=0))), \
             patch.object(report, "record") as grade:
            state = daily.update(self.base)
        self.assertEqual(state["last_succeeded_at"], "2026-10-06T17:15:00Z")
        self.assertEqual(state["repositories"], list(report.REPOS))
        self.assertFalse(report.load(directory / "current.json")["daily_policy"]["active_prs_assessed"])
        grade.assert_not_called()

    def test_reopened_assessment_is_retained_on_disk_but_excluded_from_report(self):
        directory = self.base / "terminal-evidence"
        report.save(directory / "current.json", dict(prs=[], collected_at="historic"))
        historical = dict(pr_url="https://github.com/metric-space-ai/ctox/pull/1",
                          role="parent", actor_id="old-actor", first=None, corrected=None,
                          parent_completion=dict(weighted_total=8))
        with patch.object(report, "assessments", return_value=[historical]), \
             patch.object(report, "jobs", return_value=[]), \
             patch.object(report, "read", side_effect=lambda p: "__REPORT_DATA__" if Path(p).name == "terminal_report.html" else Path(p).read_text()):
            data = report.build(self.base)
        self.assertEqual(data["assessments"], [])
        self.assertEqual(data["summary"]["unified_assessed_prs"], 0)
        self.assertEqual(historical["parent_completion"]["weighted_total"], 8)

    def test_same_head_reclosure_does_not_reuse_late_saved_old_grade(self):
        first = self.change(9, closed="2026-10-06T01:00:00Z")
        second = self.change(9, closed="2026-10-06T02:00:00Z")
        before = dict(prs=[self.pr(first)], collected_at="historic")
        with patch.object(daily, "detail", return_value=self.pr(second)):
            after, _ = daily.reconcile(self.base, before, self.changes(second),
                                       self.since, "2026-10-06T03:00:00Z")
        current = after["prs"][0]
        self.assertEqual(current["previous_terminal_events"],
                         [report.terminal_event(self.pr(first))])
        old = dict(pr_url=current["url"], role="parent", actor_id="old-parent",
                   pr_head=first["head"]["sha"], disposition="MERGED",
                   recorded_at="2026-10-06T04:00:00Z", first=None, corrected=None,
                   parent_completion=dict(weighted_total=8))
        # A retrospective review saved after the new closure still belongs to T1.
        self.assertFalse(report.current_assessment(old, current))
        self.assertFalse(report.current_assessment(
            dict(old, terminal_event=report.terminal_event(self.pr(first))), current))
        self.assertTrue(report.current_assessment(
            dict(old, terminal_event=report.terminal_event(current)), current))
        report.save(self.base / "terminal-evidence/current.json", after)
        legacy = dict(pr_url=current["url"], thread_id="old-worker",
                      first_rating=dict(score=8, pr_head=first["head"]["sha"]))
        with patch.object(report, "assessments", return_value=[old]), \
             patch.object(report, "jobs", return_value=[legacy]), \
             patch.object(report, "read", side_effect=lambda p: "__REPORT_DATA__" if Path(p).name == "terminal_report.html" else Path(p).read_text()):
            result = report.build(self.base)
        self.assertEqual(result["assessments"], [])
        self.assertEqual(result["legacy"], [])
        self.assertEqual(result["historical_assessments"], [old])
        self.assertEqual(len(result["historical_legacy"]), 1)
        self.assertEqual(result["summary"]["unified_assessed_prs"], 0)
        self.assertEqual(old["parent_completion"]["weighted_total"], 8)

    def test_reopen_history_survives_until_later_reclosure(self):
        first = self.change(9, closed="2026-10-06T01:00:00Z")
        before = dict(prs=[self.pr(first)], collected_at="historic")
        opened, _ = daily.reconcile(self.base, before,
            self.changes(self.change(9, state="open")), self.since, "open-run")
        self.assertEqual(opened["prs"], [])
        second = self.change(9, closed="2026-10-06T02:00:00Z", head="b" * 40)
        with patch.object(daily, "detail", return_value=self.pr(second)):
            reclosed, _ = daily.reconcile(self.base, opened, self.changes(second),
                                         self.since, "close-run")
        self.assertEqual(reclosed["prs"][0]["previous_terminal_events"],
                         [report.terminal_event(self.pr(first))])

    def test_record_rejects_changed_live_event_and_unbound_reclosure(self):
        old = self.pr(self.change(9, closed="2026-10-06T01:00:00Z"))
        current = self.pr(self.change(9, closed="2026-10-06T02:00:00Z"))
        current["previous_terminal_events"] = [report.terminal_event(old)]
        report.save(self.base / "terminal-evidence/current.json", dict(prs=[current]))
        entry = dict(pr_url=current["url"])
        with patch.object(report, "command", return_value=json.dumps(report.terminal_event(old))):
            with self.assertRaisesRegex(ValueError, "terminal event changed"):
                report.record(self.base, dict(entry))
        with patch.object(report, "command", return_value=json.dumps(report.terminal_event(current))):
            with self.assertRaisesRegex(ValueError, "explicit current terminal-event binding"):
                report.record(self.base, dict(entry))
            with self.assertRaisesRegex(ValueError, "different terminal event"):
                report.record(self.base, dict(entry, terminal_event=report.terminal_event(old)))

    def test_full_collection_preserves_event_history_and_verified_closing_metadata(self):
        first = self.pr(self.change(9, closed="2026-10-06T01:00:00Z"))
        second = self.pr(self.change(9, closed="2026-10-06T02:00:00Z"))
        prior = dict(second, closing_review=dict(model="verified-model", turn_id="closing-turn"))
        history = {second["url"]: [report.terminal_event(first)]}
        report.save(self.base / "terminal-evidence/current.json",
                    dict(prs=[prior], terminal_event_history=history))
        response = dict(data=dict(repository=dict(pullRequests=dict(
            totalCount=1, pageInfo=dict(hasNextPage=False), nodes=[second]))))
        with patch.object(report, "jobs", return_value=[]), \
             patch.object(report, "command", return_value=json.dumps(response)):
            result = report.collect(self.base, repositories=[report.REPOS[0]])
        self.assertEqual(result["terminal_event_history"], history)
        self.assertEqual(result["prs"][0]["closing_review"], prior["closing_review"])
        self.assertEqual(result["prs"][0]["previous_terminal_events"], history[second["url"]])

    def test_bootstrap_includes_all_five_repository_inputs(self):
        names = ["ctox", "workjet", "ctox-dev", "greppy", "miltonticket-app"]
        self.assertEqual(len(report.REPOS), 5)
        for number, name in enumerate(names, 1):
            report.save(self.base / (name + "-terminal-prs.json"),
                        [self.pr(self.change(number))])
        with patch.object(report, "build", side_effect=lambda b: report.load(b / "terminal-evidence/current.json")):
            result = report.bootstrap(self.base)
        self.assertEqual({p["repository"] for p in result["prs"]}, set(report.REPOS))
        self.assertEqual(len(result["prs"]), 5)
        self.assertEqual({p["project"] for p in result["prs"]},
                         {"CTOX", "Workjet", "ctox-dev", "Greppy", "miltonticket-app"})

    def test_naive_time_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "UTC offset"):
            daily.instant("2026-10-06T00:00:00")


if __name__ == "__main__":
    unittest.main()

