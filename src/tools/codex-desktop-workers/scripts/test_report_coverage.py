"""Observed coverage remains distinct from paired comparisons."""
import unittest
import terminal_report as report

class CoverageTests(unittest.TestCase):
    def worker(self, url, first, end, comparable=True):
        return dict(pr_url=url, actor_id=url, role="worker", schema=report.RUBRIC,
                    model="gpt-6.1-sol", harness="codex", rework_iterations=0,
                    first_end_scope_comparable=comparable,
                    first=None if first is None else dict(weighted_total=first),
                    corrected=None if end is None else dict(weighted_total=end))

    def test_partial_scores_are_visible_without_claiming_a_paired_comparison(self):
        board = report.leaderboard_data([
            self.worker("paired", 2, 8), self.worker("end-only", None, 4)
        ])[0]
        self.assertEqual(board["prs"], 1)
        self.assertEqual(board["observed_prs"], 2)
        self.assertEqual(board["first"]["mean"], 2)
        self.assertEqual(board["corrected"]["mean"], 8)
        self.assertEqual(board["observed_first"]["n"], 1)
        self.assertEqual(board["observed_corrected"]["n"], 2)
        self.assertEqual(board["observed_corrected"]["mean"], 6)

    def test_scope_changes_keep_observations_outside_matched_comparisons(self):
        board = report.leaderboard_data([self.worker("changed", 4, 8, False)])[0]
        self.assertEqual(board["prs"], 0)
        self.assertEqual(board["comparison_excluded"], 1)
        self.assertEqual(board["observed_prs"], 1)
        self.assertEqual(board["observed_first"]["mean"], 4)
        self.assertEqual(board["observed_corrected"]["mean"], 8)

    def test_parent_history_is_not_counted_as_worker_coverage(self):
        parent = dict(pr_url="parent", actor_id="parent", role="parent",
                      first=dict(weighted_total=8), corrected=dict(weighted_total=8),
                      parent_completion=dict(weighted_total=7))
        rows = [parent, self.worker("worker", None, 6)]
        coverage = report.coverage_data([
            dict(url="parent"), dict(url="worker"), dict(url="pending"),
            dict(url="stopped", stop=True)], rows)
        self.assertEqual(coverage["scored_prs"], 2)
        self.assertEqual(coverage["pending_prs"], 1)
        self.assertEqual(coverage["stopped_prs"], 1)
        self.assertEqual(coverage["roles"]["worker"]["stages"]["first"]["n"], 0)
        self.assertEqual(coverage["roles"]["worker"]["stages"]["corrected"]["n"], 1)
        self.assertEqual(coverage["roles"]["parent"]["stages"]["parent_completion"]["n"], 1)

if __name__ == "__main__":
    unittest.main()

