import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'src/scripts/build-cache.py'
spec = importlib.util.spec_from_file_location('build_cache', SCRIPT)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class CacheTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.cache = Path(self.temp.name)
        self.root = self.cache / 'builds'
        self.root.mkdir()
        self.settings = {'completed_days': 7, 'soft_bytes': 10*m.GIB, 'min_free_bytes': 1}
        self.now = time.time()

    def entry(self, name, age=0, state='completed'):
        p = self.root / name
        p.mkdir()
        (p / 'artifact').write_bytes(b'x'*8192)
        m.save(p, {'schema': 1, 'kind': 'disposable_build', 'owner': 'test-owner',
                   'state': state, 'completed_at': self.now-age*86400})
        return p

    def test_expiry_dry_run_and_apply_preserve_active_unknown_and_backups(self):
        self.entry('old', 8)
        self.entry('recent', 1)
        self.entry('active', 30, 'active')
        unknown = self.root / 'unknown'; unknown.mkdir()
        backup = self.cache/'backups'; backup.mkdir(); (backup/'recovery').write_text('retain')
        dry = m.sweep(self.root, self.settings, now=self.now)
        self.assertEqual(dry['selected'], ['old'])
        self.assertTrue((self.root/'old').exists())
        applied = m.sweep(self.root, self.settings, True, self.now)
        self.assertEqual(applied['removed'], ['old'])
        self.assertTrue((self.root/'recent').exists())
        self.assertTrue((self.root/'active').exists())
        self.assertTrue(unknown.exists())
        self.assertEqual((backup/'recovery').read_text(), 'retain')
        self.assertEqual(m.sweep(self.root,self.settings,True,self.now)['removed'], [])

    def test_soft_cap_oldest_first_and_protected_excess_reported(self):
        self.entry('older', 2); self.entry('newer', 1); self.entry('active', 9, 'active')
        self.settings['soft_bytes'] = 1
        r=m.sweep(self.root,self.settings,True,self.now)
        self.assertEqual(r['removed'], ['older','newer'])
        self.assertTrue(r['over_soft_limit'])
        self.assertTrue((self.root/'active').exists())

    def test_future_metadata_never_expires(self):
        self.entry('future', -1)
        self.settings['soft_bytes']=1
        self.assertEqual(m.sweep(self.root,self.settings,True,self.now)['removed'], [])

    def test_malformed_metadata_fails_closed(self):
        p=self.entry('bad',8); (p/m.MARKER).write_text('{bad')
        with self.assertRaises(ValueError):m.sweep(self.root,self.settings,True,self.now)
        self.assertTrue(p.exists())

    def test_symlink_inside_entry_never_traversed_or_deleted(self):
        p=self.entry('old',8); outside=self.cache/'valuable'; outside.write_text('keep')
        (p/'escape').symlink_to(outside)
        with self.assertRaises(ValueError):m.sweep(self.root,self.settings,True,self.now)
        self.assertEqual(outside.read_text(),'keep')
        self.assertTrue(p.exists())

    def test_live_lease_blocks_second_sweep(self):
        with m.locked(self.cache):
            r=subprocess.run([sys.executable,str(SCRIPT),'--cache-root',str(self.cache),'sweep','--apply'],capture_output=True,text=True)
        self.assertNotEqual(r.returncode,0)
        self.assertIn('busy',r.stderr)

    def test_disk_floor_prevents_command_and_entry_creation(self):
        with m.locked(self.cache) as (root,fd):
            with patch.object(m.shutil,'disk_usage',return_value=type('Usage',(),{'free':0})()):
                with self.assertRaisesRegex(ValueError,'denied'):
                    m.run(root,fd,self.settings,'owner','new',self.cache,[sys.executable,'-c','raise RuntimeError()'],2)
        self.assertFalse((self.root/'new').exists())

    def test_command_completion_registers_only_new_owned_cache(self):
        with m.locked(self.cache) as (root,fd):
            self.assertEqual(m.run(root,fd,self.settings,'owner','new',self.cache,
                [sys.executable,'-c','import os,pathlib; pathlib.Path(os.environ["CARGO_TARGET_DIR"]).mkdir()'],3),0)
        metadata=json.loads((self.root/'new'/m.MARKER).read_text())
        self.assertEqual(metadata['state'],'completed')
        self.assertTrue((self.root/'new'/'target').is_dir())
        with m.locked(self.cache) as (root,fd):
            with self.assertRaisesRegex(ValueError,'already exists'):
                m.run(root,fd,self.settings,'owner','new',self.cache,['false'],2)

    def test_traversal_name_rejected(self):
        with m.locked(self.cache) as (root,fd):
            with self.assertRaises(ValueError):m.run(root,fd,self.settings,'owner','../outside',self.cache,['false'],2)

    def test_removal_failure_is_reported_and_not_counted_as_reclaimed(self):
        self.entry('old',8)
        original=m.shutil.rmtree
        def deny(*a,**kw):raise PermissionError('test denial')
        deny.avoids_symlink_attacks=original.avoids_symlink_attacks
        with patch.object(m.shutil,'rmtree',deny):r=m.sweep(self.root,self.settings,True,self.now)
        self.assertEqual(len(r['errors']),1)
        self.assertEqual(r['bytes_before'],r['bytes_after'])
        self.assertTrue((self.root/'old').exists())

    def test_installer_entry_point_blocks_under_configured_floor(self):
        (self.cache/'build-cache-policy.json').write_text(json.dumps({'min_free_bytes':10**18}))
        repo=SCRIPT.parents[2]
        command='source "$1/install.sh"; CACHE_ROOT="$2"; SCRIPT_DIR="$1"; run_build_module test "$2" touch "$2/ran"'
        r=subprocess.run(['bash','-c',command,'bash',str(repo),str(self.cache)],capture_output=True,text=True)
        self.assertNotEqual(r.returncode,0)
        self.assertIn('heavy build denied',r.stderr)
        self.assertFalse((self.cache/'ran').exists())

    def test_root_symlink_and_policy_typo_rejected(self):
        self.root.rmdir()
        outside=self.cache/'other'; outside.mkdir()
        self.root.symlink_to(outside)
        with self.assertRaises(ValueError):
            with m.locked(self.cache):pass
        (self.cache/'build-cache-policy.json').write_text('{"soft_btyes":1}')
        with self.assertRaises(ValueError):m.policy(self.cache)

    def test_installer_propagates_child_failure_in_conditional(self):
        (self.cache/'build-cache-policy.json').write_text(json.dumps({'min_free_bytes':1}))
        repo=SCRIPT.parents[2]
        script='source "$1/install.sh"; CACHE_ROOT="$2"; SCRIPT_DIR="$1"; if run_build_module test "$2" false; then exit 99; else exit 0; fi'
        r=subprocess.run(['bash','-c',script,'bash',str(repo),str(self.cache)],capture_output=True,text=True)
        self.assertEqual(r.returncode,0,r.stderr)

    def test_timeout_leaves_entry_protected_and_releases_lease(self):
        with m.locked(self.cache) as (root, fd):
            with self.assertRaises(subprocess.TimeoutExpired):
                m.run(root, fd, self.settings, 'owner', 'timed', self.cache,
                      [sys.executable, '-c', 'import time; time.sleep(60)'], 0.05)
        meta=json.loads((self.root/'timed'/m.MARKER).read_text())
        self.assertEqual(meta['state'], 'active')
        with m.locked(self.cache) as (root, fd):
            self.assertEqual(m.sweep(root, self.settings, True)['removed'], [])


if __name__ == '__main__':unittest.main()
