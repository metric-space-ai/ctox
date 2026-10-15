import hashlib
import json
import os
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import model_draw
import worker

class StarterDrawTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(dir=os.environ['TMPDIR'])
        self.addCleanup(self.folder.cleanup)
        self.base = Path(self.folder.name)
        self.jobs = self.base/'jobs'
        self.jobs.mkdir()
        self.brief = self.base/'brief.md'
        self.brief.write_bytes(b'Implement the bounded package.\r\n')
        self.args = SimpleNamespace(parent_thread='parent', issue='https://github.com/o/r/issues/1',
                                    prompt_file=self.brief, model=None, reasoning='high')
        self.identity = hashlib.sha256(('parent\n'+self.args.issue).encode()).hexdigest()
        self.choice = dict(schema='worker-model-draw-v1', selection_id=self.identity,
                           model_id='gpt-6.1-sol', provider='openai', harness='codex',
                           route_id='verified-native-route', mode='uniform',
                           execution=dict(app_server_provider='openai', supported_reasoning=['high'],
                                          route_evidence='synthetic unit-test export'),
                           actual_authoring_model=None, worker_id=None, pr_url=None)
    def test_unready_pool_stops_create_before_worktree_or_process(self):
        (self.base/'proxy-workers').mkdir()
        (self.base/'proxy-workers/MODEL-POOL.json').write_text(json.dumps(dict(schema=model_draw.SCHEMA,status='awaiting-export')))
        with patch.object(worker,'HOME',self.base), patch.object(worker,'JOBS',self.jobs), \
             patch.object(worker,'validate_worktree') as worktree, patch.object(worker.subprocess,'Popen', wraps=worker.subprocess.Popen) as spawn:
            with self.assertRaisesRegex(ValueError,'not ready'): worker.create(self.args)
            worktree.assert_not_called()
            self.assertTrue(all(call.args[0][0] == "greppy" for call in spawn.call_args_list))
        self.assertIsNone(self.args.model)
    def test_new_assignment_draws_without_classification_or_model_default(self):
        with patch.object(model_draw,'read',return_value={}), patch.object(model_draw,'draw',return_value=self.choice) as draw:
            selection=model_draw.selection_for_create(self.args,self.base,self.jobs)
        self.assertEqual(draw.call_args.args[3],hashlib.sha256(self.brief.read_bytes()).hexdigest())
        self.assertEqual(selection['mode'],'uniform')
        self.assertEqual(self.args.model,'gpt-6.1-sol')
        self.assertIsNone(selection['actual_authoring_model'])
    def test_explicit_model_cannot_filter_or_override_the_draw(self):
        self.args.model='gpt-6-astra'
        with patch.object(model_draw,'read',return_value={}), patch.object(model_draw,'draw',return_value=self.choice) as draw:
            with self.assertRaisesRegex(ValueError,'cannot override'):model_draw.selection_for_create(self.args,self.base,self.jobs)
            draw.assert_called_once()
    def test_foreign_home_harness_never_gets_codex_proxy_fallback(self):
        choice={**self.choice,'model_id':'claude-opus-5-5','provider':'anthropic','harness':'claude-code'}
        with patch.object(model_draw,'read',return_value={}),patch.object(model_draw,'draw',return_value=choice):
            with self.assertRaisesRegex(ValueError,'no Codex proxy fallback'):model_draw.selection_for_create(self.args,self.base,self.jobs)
    def test_existing_or_archived_package_cannot_get_another_worker(self):
        for archived in (None,'2026-10-13'):
            (self.jobs/'old.json').write_text(json.dumps(dict(thread_id='old',parent_thread='parent',
                issue_url=self.args.issue,archived_at=archived)))
            with patch.object(model_draw,'draw') as draw:
                with self.assertRaisesRegex(ValueError,'already owns task old'):model_draw.selection_for_create(self.args,self.base,self.jobs)
                draw.assert_not_called()
    def test_legacy_assignment_never_redraws(self):
        (self.base/'selections').mkdir()
        (self.base/'selections'/(self.identity+'.json')).write_text('{}')
        with patch.object(model_draw,'draw') as draw:
            with self.assertRaisesRegex(ValueError,'Legacy assignment'):model_draw.selection_for_create(self.args,self.base,self.jobs)
            draw.assert_not_called()
    def test_unsupported_effort_retains_the_draw_without_fallback(self):
        self.args.reasoning='max'
        with patch.object(model_draw,'read',return_value={}),patch.object(model_draw,'draw',return_value=self.choice):
            with self.assertRaisesRegex(ValueError,'retain the draw'):model_draw.selection_for_create(self.args,self.base,self.jobs)
    def test_missing_native_route_is_rejected(self):
        with patch.object(model_draw,'read',return_value={}),patch.object(model_draw,'draw',return_value={**self.choice,'execution':{}}):
            with self.assertRaisesRegex(ValueError,'verified native Codex route'):model_draw.selection_for_create(self.args,self.base,self.jobs)
if __name__=='__main__':unittest.main()
