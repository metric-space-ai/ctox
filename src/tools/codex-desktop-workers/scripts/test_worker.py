import importlib.util
import datetime as dt
import json
import os
import shutil
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch
from types import SimpleNamespace

spec = importlib.util.spec_from_file_location('worker', Path(__file__).with_name('worker.py'))
w = importlib.util.module_from_spec(spec)
spec.loader.exec_module(w)


class WorkerGuards(unittest.TestCase):
    def test_parent_canonical_project_wins_over_working_directory(self):
        request = Mock(return_value={'thread': {'projectId': 'parent-project', 'cwd': '/different/repository'}})
        self.assertEqual(w.resolve_parent_project(request, 'parent'), 'parent-project')
        request.assert_called_once_with(10, 'thread/read', {'threadId': 'parent', 'includeTurns': False})

    def test_parent_project_fallback_paginates_and_normalizes_exact_root(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            root = Path(directory)
            request = Mock(side_effect=[
                {'thread': {'projectId': None, 'cwd': str(root / 'repo' / '..')}},
                {'data': [{'id': 'other', 'roots': [{'path': str(root / 'child')}]}], 'nextCursor': 'page2'},
                {'data': [{'id': 'match', 'roots': [{'path': str(root)}]}], 'nextCursor': None},
            ])
            self.assertEqual(w.resolve_parent_project(request, 'parent'), 'match')
            self.assertEqual(request.call_args_list[-1].args[2]['cursor'], 'page2')

    def test_parent_project_fallback_rejects_ambiguous_or_missing_membership(self):
        for ids in [[], ['first', 'second']]:
            request = Mock(side_effect=[
                {'thread': {'cwd': '/parent/root'}},
                {'data': [{'id': project, 'roots': [{'path': '/parent/root'}]} for project in ids]},
            ])
            with self.subTest(ids=ids), self.assertRaisesRegex(ValueError, 'Assign the parent to exactly one'):
                w.resolve_parent_project(request, 'parent')

    def test_failed_preparation_retains_registry_and_stops_setup_process(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            root = Path(directory)
            jobs = root / 'jobs'
            output = Path('/Volumes/tmp/dev-artifacts') / root.name
            self.addCleanup(shutil.rmtree, output)
            thread = '00000000-0000-0000-0000-000000000001'
            registry = jobs / (thread + '.json')
            args = SimpleNamespace(model='kimi-k3', repo=root, worktree=root / 'worktree',
                                   issue='https://github.com/owner/repo/issues/1',
                                   parent_thread='parent', parent_title='Parent', title='Assignment',
                                   prompt_file=root / 'assignment.md', reasoning='high')
            client = Mock(turn_id='turn-1')
            client.request.side_effect = [{}, {'thread': {'projectId': 'parent-project'}},
                {'model': 'kimi-k3', 'thread': {'id': thread, 'projectId': 'parent-project',
                 'modelProvider': 'cli_proxy', 'path': str(root / 'missing.jsonl')}}, {}]

            def incomplete_turn(*_):
                self.assertEqual(json.loads(registry.read_text())['preparation_status'], 'preparing')
                raise TimeoutError('Task preparation deadline exceeded')

            client.prepare.side_effect = incomplete_turn
            process = Mock(pid=12345)
            with patch.object(w, 'JOBS', jobs), patch.object(w, 'HOME', root), \
                 patch.object(w, 'AVAILABILITY', root / 'missing-availability.json'), \
                 patch.object(w, 'validate_worktree', return_value=(root, args.worktree, 'codex/prep', 'owner/repo')), \
                 patch.object(w, 'read', side_effect=lambda p: '' if p.name == 'config.toml' else 'Bounded assignment'), \
                 patch.object(w, 'run', return_value=json.dumps({'url': args.issue, 'state': 'OPEN'})), \
                 patch.object(w.context_config, 'install', return_value=str(root / '.codex/config.toml')), \
                 patch.object(w.context_config, 'verify'), \
                 patch.object(w.subprocess, 'Popen', return_value=process), \
                 patch.object(w.selectors, 'DefaultSelector'), \
                 patch.object(w.preparation, 'Client', return_value=client):
                with self.assertRaisesRegex(RuntimeError, 'existing task retained'):
                    w.create(args)
            saved = json.loads(registry.read_text())
            self.assertEqual(saved['preparation_status'], 'failed')
            self.assertEqual(saved['thread_id'], thread)
            self.assertEqual(saved['project_id'], 'parent-project')
            start = next(call for call in client.request.call_args_list if call.args[1] == 'thread/start')
            self.assertEqual(start.args[2]['projectId'], 'parent-project')
            self.assertEqual(start.args[2]['historyMode'], 'paginated')
            self.assertEqual(start.args[2]['config']['model_context_window'], 256000)
            self.assertEqual(start.args[2]['config']['model_auto_compact_token_limit'], 230400)
            self.assertEqual(saved['preparation_turn_id'], 'turn-1')
            self.assertIn('Do not rerun create', saved['recovery'])
            client.interrupt.assert_called_once_with(thread)
            process.terminate.assert_called_once()
            process.wait.assert_called_once_with(timeout=5)

    def test_operator_experience_is_attributed_and_survives_review_refresh(self):
        prior = {'kimi-k3': {'source': 'Operator', 'date': '2026-09-09',
                  'use_for': 'UI review', 'observation': 'Strong design feedback',
                  'review_focus': 'Verify actual UI behavior'}}
        rendered = w.experience.render([], prior)
        self.assertIn('Source: Operator', rendered)
        self.assertIn('UI review', rendered)
        self.assertIn('not a completed PR evaluation', rendered)

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
