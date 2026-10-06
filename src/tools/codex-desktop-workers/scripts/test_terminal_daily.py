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

    def test_naive_time_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "UTC offset"):
            daily.instant("2026-10-06T00:00:00")


if __name__ == "__main__":
    unittest.main()

