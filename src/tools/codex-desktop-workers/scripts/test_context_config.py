import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock
import context_config as c


def run(*args, cwd):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()


class ContextConfig(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=os.environ['TMPDIR'])
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        run('git', 'init', '-q', cwd=self.root)
        self.catalog = self.root / 'private-catalog.json'

    def test_private_budget_is_ignored_and_idempotent(self):
        path = Path(c.install(self.root, self.catalog, run))
        self.assertIn('model_context_window = 256000', path.read_text())
        self.assertIn('model_auto_compact_token_limit = 230400', path.read_text())
        self.assertEqual(run('git', 'status', '--porcelain', cwd=self.root), '')
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assertEqual(c.install(self.root, self.catalog, run), str(path))

    def test_existing_user_config_is_never_overwritten(self):
        path = self.root / '.codex/config.toml'
        path.parent.mkdir()
        path.write_text('custom = true\n')
        with self.assertRaisesRegex(ValueError, 'must not be overwritten'):
            c.install(self.root, self.catalog, run)
        self.assertEqual(path.read_text(), 'custom = true\n')

    def test_tracked_config_is_rejected(self):
        path = self.root / '.codex/config.toml'
        path.parent.mkdir()
        path.write_text('custom = true\n')
        run('git', 'add', '.codex/config.toml', cwd=self.root)
        with self.assertRaisesRegex(ValueError, 'tracked'):
            c.install(self.root, self.catalog, run)

    def test_symlink_directory_is_rejected_without_creating_files(self):
        target = self.root / 'elsewhere'
        target.mkdir()
        (self.root / '.codex').symlink_to(target, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            c.install(self.root, self.catalog, run)
        self.assertEqual(list(target.iterdir()), [])

    def test_ignored_or_wrong_effective_budget_blocks_dispatch(self):
        for config in [{}, {'model_context_window': 272000},
                       {'model_context_window': 256000, 'model_auto_compact_token_limit': 250000}]:
            with self.subTest(config=config), self.assertRaisesRegex(ValueError, 'not effective'):
                c.verify(Mock(return_value={'config': config}), self.root)

    def test_effective_budget_is_checked_at_worker_cwd(self):
        request = Mock(return_value={'config': {'model_context_window': 256000,
                                               'model_auto_compact_token_limit': 230400}})
        c.verify(request, self.root)
        self.assertEqual(request.call_args.args[2]['cwd'], str(self.root))


if __name__ == '__main__':
    unittest.main()
