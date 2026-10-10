"""Exercise real framed writer/reader subprocesses in an isolated local target."""
import hashlib
import json
from pathlib import Path
import stat
import struct
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import terminal_evidence_storage as storage
import terminal_report as report


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode()


def framed(raw, footer=None):
    digest = hashlib.sha256(raw).hexdigest()
    return (struct.pack('>I', len(raw)) + raw + struct.pack('>I', 0) +
            (json.dumps(footer or dict(bytes=len(raw), sha256=digest)) + '\n').encode())


class StorageTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.folder = Path(self.scratch.name)
        self.root = self.folder/'evidence'
        self.remote = self.folder/'remote'
        self.root.mkdir()
        self.remote.mkdir()
        self.settings = patch.multiple(storage, ROOT=self.root, DEST=str(self.remote),
            MANIFEST=self.root/'moved.tsv', OUTPUTS=self.root/'outputs.tsv',
            SSH=['/bin/sh', '-c'], LIMIT=100)
        self.settings.start()
        self.addCleanup(self.settings.stop)

    def writer(self, filename, raw, expected=None, prefix=''):
        args = json.dumps(dict(root=str(self.remote), file=filename, expected=expected))
        return subprocess.run([sys.executable, '-c', prefix+storage._REMOTE_WRITE, args],
            input=raw, capture_output=True, timeout=10)

    def test_byte_threshold_includes_unicode_and_final_newline(self):
        value = {'message': 'ä'*25}
        size = len(encoded(value))
        with patch.object(storage, 'LIMIT', size):
            row = storage.save_json(self.root/'local.json', value)
            self.assertTrue(row['local'])
            self.assertEqual((self.root/'local.json').read_bytes(), encoded(value))
            self.assertEqual(stat.S_IMODE((self.root/'local.json').stat().st_mode), 0o600)
        with patch.object(storage, 'LIMIT', size-1):
            row = storage.save_json(self.root/'remote.json', value)
            self.assertFalse(row['local'])
            self.assertFalse((self.root/'remote.json').exists())
            self.assertEqual((self.remote/'remote.json').read_bytes(), encoded(value))
            self.assertEqual(storage.read_text(self.root/'remote.json'), encoded(value).decode())

    def test_direct_raw_roundtrip_keeps_previous_version_and_no_local_cache(self):
        path = self.root/'nested/raw.json'
        original = {'step': 1}
        first = storage.save_json(path, original, force_remote=True)
        final = {'step': 2}
        second = storage.save_json(path, final, force_remote=True)
        self.assertFalse(path.exists())

        self.assertEqual(second['previous']['sha256'], first['sha256'])
        self.assertEqual(Path(second['previous']['target_path']).read_bytes(), encoded(original))
        self.assertEqual(json.loads(storage.read_text(path)), final)
        self.assertFalse(path.exists())
        history = json.loads((self.root/'GPU3-RAW-VERSIONS.jsonl').read_text())
        self.assertEqual(history['sha256'], first['sha256'])
        (self.remote/'nested/raw.json').write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
            storage.read_text(path)
        self.assertFalse(path.exists())

    def test_first_raw_write_creates_only_the_local_manifest_directory(self):
        self.root.rmdir()
        path = self.root/'first.json'
        storage.save_json(path, {'first': True}, force_remote=True)
        self.assertTrue(storage.OUTPUTS.is_file())
        self.assertFalse(path.exists())
        self.assertEqual(json.loads(storage.read_text(path)), {'first': True})

    def test_existing_local_raw_is_preserved_and_outside_paths_rejected(self):
        path = self.root/'existing.json'
        path.write_bytes(b'original local evidence')
        with self.assertRaisesRegex(ValueError, 'verified/offloaded'):
            storage.save_json(path, {'new': 1}, force_remote=True)
        with self.assertRaisesRegex(ValueError, 'verified/offloaded'):
            storage.save_json(path, {'large': 'x'*200})
        self.assertEqual(path.read_bytes(), b'original local evidence')
        with self.assertRaisesRegex(ValueError, 'restricted'):
            storage.save_json(self.folder/'outside.json', {'new': 1}, force_remote=True)

    def test_untracked_or_missing_remote_destinations_are_never_overwritten(self):
        (self.remote/'untracked.json').write_bytes(b'original')
        with self.assertRaisesRegex(RuntimeError, 'refusing overwrite'):
            storage.save_json(self.root/'untracked.json', {'new': 1}, force_remote=True)
        self.assertEqual((self.remote/'untracked.json').read_bytes(), b'original')
        self.assertIsNone(storage.locator(self.root/'untracked.json'))
        storage.save_json(self.root/'missing.json', {'first': 1}, force_remote=True)
        (self.remote/'missing.json').unlink()
        with self.assertRaisesRegex(RuntimeError, 'missing; refusing replacement'):
            storage.save_json(self.root/'missing.json', {'second': 2}, force_remote=True)
        self.assertFalse((self.remote/'missing.json').exists())

    def test_truncation_checksum_and_oversized_frames_preserve_previous_bytes(self):
        old = b'previous evidence'
        path = self.remote/'protected.json'
        sha = hashlib.sha256(old).hexdigest()
        cases = [struct.pack('>I', 5)+b'ab',
                 framed(b'new', dict(bytes=3, sha256='0'*64)),
                 struct.pack('>I', 1048577)]
        for raw in cases:
            with self.subTest(stream=raw[:12]):
                path.write_bytes(old)
                result = self.writer('protected.json', raw, sha)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(path.read_bytes(), old)
                self.assertFalse(list(self.remote.glob('*.writing')))

    def test_rejected_receipt_is_not_added_to_manifest(self):
        def altered(rel, expected):
            # Corrupt only the stdout receipt; preserve the genuine writer and stream.
            args = json.dumps(dict(root=str(self.remote), file=rel, expected=expected))
            prefix = ("import json\n_original_dump=json.dump\n"
                "def changed_dump(v,f):\n v['bytes']+=1;_original_dump(v,f)\njson.dump=changed_dump\n")
            return subprocess.Popen([sys.executable,'-c',prefix+storage._REMOTE_WRITE,args],
                stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        with patch.object(storage, '_process', altered):
            with self.assertRaisesRegex(ValueError, 'receipt does not match'):
                storage.save_json(self.root/'receipt.json', {'data': 1}, force_remote=True)
        self.assertIsNone(storage.locator(self.root/'receipt.json'))
        self.assertFalse((self.root/'receipt.json').exists())

    def test_concurrent_stale_writer_cannot_replace_unrecorded_new_version(self):
        path = self.remote/'shared.json'
        old, first, second = b'previous', b'first writer', b'stale second writer'
        path.write_bytes(old)
        expected = hashlib.sha256(old).hexdigest()
        marker, release = self.folder/'held', self.folder/'release'
        prefix = ("import os,time\nfrom pathlib import Path\n_original_replace=os.replace\n"
                  "def delayed_replace(a,b):\n"
                  f" Path({str(marker)!r}).touch()\n"
                  " deadline=time.monotonic()+8\n"
                  f" while not Path({str(release)!r}).exists():\n"
                  "  if time.monotonic()>deadline:raise TimeoutError('test barrier')\n"
                  "  time.sleep(.01)\n _original_replace(a,b)\nos.replace=delayed_replace\n")
        args = json.dumps(dict(root=str(self.remote),file='shared.json',expected=expected))
        one = subprocess.Popen([sys.executable,'-c',prefix+storage._REMOTE_WRITE,args],
            stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        two = None
        try:
            one.stdin.write(framed(first));one.stdin.close();one.stdin=None
            deadline = time.monotonic()+5
            while not marker.exists() and time.monotonic()<deadline:
                time.sleep(.01)
            self.assertTrue(marker.exists())
            two = subprocess.Popen([sys.executable,'-c',storage._REMOTE_WRITE,args],
                stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            two.stdin.write(framed(second));two.stdin.close();two.stdin=None
            release.touch()
            one.communicate(timeout=10)
            _, error = two.communicate(timeout=10)
            self.assertEqual(one.returncode, 0)
            self.assertNotEqual(two.returncode, 0)
            self.assertIn(b'changed destination; refusing overwrite', error)
            self.assertEqual(path.read_bytes(), first)
            self.assertEqual((self.remote/'.versions/shared.json'/f'{expected}.raw').read_bytes(), old)
        finally:
            release.touch()
            for child in (one,two):
                if child is not None and child.poll() is None:
                    child.terminate();child.communicate(timeout=10)

    def test_report_routes_evidence_and_keeps_report_data_local(self):
        raw = self.root/'raw.json'
        report.save_raw(raw, {'retained': True})
        self.assertEqual(report.load(raw), {'retained': True})
        self.assertFalse(raw.exists())
        other = self.folder/'report.json'
        report.save(other, {'ordinary': True})
        self.assertEqual(report.load(other), {'ordinary': True})
        self.assertIsNone(storage.locator(other))


if __name__ == '__main__':
    unittest.main()
