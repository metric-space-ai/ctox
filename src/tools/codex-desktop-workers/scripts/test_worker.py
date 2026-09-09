import importlib.util
import datetime as dt
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('worker', Path(__file__).with_name('worker.py'))
w = importlib.util.module_from_spec(spec)
spec.loader.exec_module(w)


class WorkerGuards(unittest.TestCase):
    def test_unchanged_confirmation_preserves_substantive_history(self):
        success = {key: 'detail' for key in w.experience.FIELDS}
        success.update(outcome='success', failure_cause='none', strengths='Verified boundary handling',
                       reviewed_at='2026-09-09T10:00:00Z', pr_url='https://github.com/o/r/pull/1')
        unchanged = {**success, 'outcome': 'reviewed_unchanged', 'failure_cause': 'quota',
                     'strengths': 'No new assessment', 'reviewed_at': '2026-09-09T11:00:00Z'}
        job = {'model': 'kimi-k3', 'learning_review': unchanged,
               'review_history': [success, unchanged]}
        rendered = w.experience.render([job])
        kimi = rendered.split('## Kimi K3')[1]
        self.assertIn('Verified boundary handling', kimi)
        self.assertNotIn('Coding strengths: not established', kimi)

    def test_unknown_quota_reset_expires_after_one_day(self):
        now = dt.datetime(2026, 9, 9, tzinfo=dt.timezone.utc)
        entry = w.availability.defer('kimi-k3', None, 'quota exhausted', now)
        self.assertTrue(w.availability.active(entry, now + dt.timedelta(hours=23)))
        self.assertFalse(w.availability.active(entry, now + dt.timedelta(days=1)))

    def test_provider_reset_takes_precedence_over_default_day(self):
        now = dt.datetime(2026, 9, 9, tzinfo=dt.timezone.utc)
        entry = w.availability.defer('kimi-k3', '2026-09-16T00:00:00Z', 'weekly reset', now)
        self.assertTrue(w.availability.active(entry, now + dt.timedelta(days=6)))
        self.assertFalse(w.availability.active(entry, now + dt.timedelta(days=7)))

    def test_quota_cannot_be_recorded_as_model_quality_failure(self):
        data = {key: 'observation' for key in w.experience.FIELDS}
        data.update(outcome='failure', failure_cause='quota')
        with self.assertRaisesRegex(ValueError, 'temporary availability'):
            w.experience.validate_assessment(data)

    def test_first_retrospective_after_merge_does_not_allow_archive(self):
        job = {'learning_review': {'pr_head': 'head'},
               'review_history': [{'pr_head': 'head', 'pr_state_at_review': 'MERGED'}]}
        pr = {'state': 'MERGED', 'mergedAt': '2026-09-09', 'headRefOid': 'head'}
        with self.assertRaisesRegex(ValueError, 'before merge'):
            w.require_archive_ready(job, pr)

    def test_archive_requires_review_of_current_merged_head(self):
        job = {'learning_review': {'pr_head': 'old'}, 'review_history': [{'pr_head': 'new', 'pr_state_at_review': 'OPEN'}]}
        pr = {'state': 'MERGED', 'mergedAt': '2026-09-09', 'headRefOid': 'new'}
        with self.assertRaisesRegex(ValueError, 'model experience'):
            w.require_archive_ready(job, pr)

    def test_dirty_worker_is_not_ready_to_archive(self):
        job = {'worktree': '/example', 'branch': 'codex/task', 'learning_review': {'pr_head': 'head'}, 'review_history': [{'pr_head': 'head', 'pr_state_at_review': 'OPEN'}]}
        pr = {'state': 'MERGED', 'mergedAt': '2026-09-09', 'headRefOid': 'head'}
        with patch.object(w, 'run', side_effect=['codex/task', 'head', '?? untracked.py']):
            with self.assertRaisesRegex(ValueError, 'uncommitted'):
                w.require_archive_ready(job, pr)

    def test_clean_reviewed_merged_worker_is_ready(self):
        job = {'worktree': '/example', 'branch': 'codex/task', 'learning_review': {'pr_head': 'head'}, 'review_history': [{'pr_head': 'head', 'pr_state_at_review': 'OPEN'}]}
        pr = {'state': 'MERGED', 'mergedAt': '2026-09-09', 'headRefOid': 'head'}
        with patch.object(w, 'run', side_effect=['codex/task', 'head', '']):
            w.require_archive_ready(job, pr)

    def test_empty_model_assessment_is_rejected(self):
        with self.assertRaises(ValueError):
            w.experience.validate_assessment({'outcome': 'success'})

    def test_experience_preserves_negative_and_positive_evidence(self):
        review = {key: 'measured detail' for key in w.experience.FIELDS}
        review.update(outcome='mixed', strengths='Passed focused tests', weaknesses='Missed a regression',
                      reviewed_at='2026-09-09', pr_url='https://github.com/o/r/pull/1')
        rendered = w.experience.render([{'model': 'kimi-k3', 'learning_review': review}])
        self.assertIn('Passed focused tests', rendered)
        self.assertIn('Missed a regression', rendered)
        self.assertIn('https://github.com/o/r/pull/1', rendered)
        self.assertIn('mixed', rendered)

    def test_worker_cannot_be_rebound_to_another_pr(self):
        job = {'pr_url': 'https://github.com/owner/repo/pull/1'}
        with patch.object(sys, 'argv', ['worker.py', 'bind-pr', '--thread', 'test', '--pr', 'https://github.com/owner/repo/pull/2']), \
             patch.object(w, 'load_job', return_value=(Path('unused'), job)), \
             patch.object(w, 'pr_info') as query:
            with self.assertRaisesRegex(ValueError, 'exactly one PR'):
                w.main()
            query.assert_not_called()

    def test_wrong_repository_pr_never_queries_github(self):
        with patch.object(w, 'run') as run:
            with self.assertRaises(ValueError):
                w.pr_info({'repository': 'owner/repo', 'branch': 'codex/task'}, 'https://github.com/other/repo/pull/1')
            run.assert_not_called()

    def test_wrong_pr_branch_rejected(self):
        with patch.object(w, 'run', return_value=json.dumps({'headRefName': 'main'})):
            with self.assertRaises(ValueError):
                w.pr_info({'repository': 'owner/repo', 'branch': 'codex/task'}, 'https://github.com/owner/repo/pull/1')

    def test_closed_unmerged_pr_cannot_mark_task_archived(self):
        job = {'pr_url': 'https://github.com/owner/repo/pull/1'}
        with patch.object(sys, 'argv', ['worker.py', 'archived', '--thread', 'test']), \
             patch.object(w, 'load_job', return_value=(Path('unused'), job)), \
             patch.object(w, 'pr_info', return_value={'state': 'CLOSED', 'mergedAt': None}), \
             patch.object(w, 'save') as save:
            with self.assertRaisesRegex(ValueError, 'has not been merged'):
                w.main()
            save.assert_not_called()

    def test_bind_rejects_unpushed_head(self):
        job = {'worktree': '/Volumes/tmp/worktrees/project/task'}
        with patch.object(sys, 'argv', ['worker.py', 'bind-pr', '--thread', 'test', '--pr', 'url']), \
             patch.object(w, 'load_job', return_value=(Path('unused'), job)), \
             patch.object(w, 'pr_info', return_value={'headRefOid': 'remote'}), \
             patch.object(w, 'run', return_value='local'), patch.object(w, 'save') as save:
            with self.assertRaisesRegex(ValueError, 'pushed PR head'):
                w.main()
            save.assert_not_called()

    def test_registry_creation_lock_is_exclusive_and_released(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as root, patch.object(w, 'JOBS', Path(root)):
            with w.job_lock('create'):
                with self.assertRaisesRegex(ValueError, 'registry lock'):
                    with w.job_lock('create'):
                        pass
            with w.job_lock('create'):
                pass

    def test_task_id_cannot_escape_registry(self):
        with self.assertRaises(ValueError):
            w.load_job('../../other')


if __name__ == '__main__':
    unittest.main()
